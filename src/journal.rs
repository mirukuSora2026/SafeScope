//! The record of what the engine did, and was about to do.
//!
//! This module carries invariant I1: **the intent record is durable before any
//! file changes**. Every state change is committed with `synchronous = FULL`
//! before the corresponding filesystem work begins, so after a crash there is
//! always a record saying what was being attempted.
//!
//! That is what makes recovery able to ask a decidable question. A crash leaves
//! an operation at [`Stage::Applying`], and because the plan recorded the exact
//! state expected on both sides, the engine can compare what is on disk against
//! each and reach one of four answers rather than a guess.
//!
//! `synchronous = FULL` is slower than the usual `NORMAL`. It is the right trade
//! here: the product is the ability to say what happened, and a journal that can
//! lose its last few records cannot.

pub mod record;

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::dataformatting::Msg;
use crate::domain::{PathState, Transition};
use crate::error::{Denial, Error, ErrorCode, Fault, Result};
use crate::hash::ContentHash;
use crate::ids::{OperationId, RequestId, TaskId};
use crate::store::StatePaths;

pub use self::record::{NewOperation, OperationKind, OperationRecord, Stage};

/// Bumped when the schema changes in a way older builds cannot read.
const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS operations (
    id             TEXT PRIMARY KEY,
    task           TEXT NOT NULL,
    plan           TEXT NOT NULL,
    kind           TEXT NOT NULL,
    reverses       TEXT,
    request        TEXT,
    request_digest TEXT,
    sequence       INTEGER NOT NULL,
    stage          TEXT NOT NULL,
    transition     TEXT NOT NULL,
    payload        TEXT,
    observed       TEXT,
    error_code     TEXT,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS operations_sequence ON operations(sequence);
CREATE INDEX IF NOT EXISTS operations_task ON operations(task, sequence);
CREATE UNIQUE INDEX IF NOT EXISTS operations_request
    ON operations(task, request) WHERE request IS NOT NULL;
";

/// The workspace's operation journal.
#[derive(Debug)]
pub struct Journal {
    connection: Connection,
}

impl Journal {
    /// Opens the journal for a workspace, creating it if needed.
    pub fn open(paths: &StatePaths) -> Result<Self> {
        Self::open_at(&paths.journal())
    }

    pub fn open_at(path: &Path) -> Result<Self> {
        let connection = Connection::open(path).map_err(|error| {
            Error::Faulted(Fault::new(
                ErrorCode::JournalFailed,
                Msg::JournalOpenFailed {
                    path: path.display().to_string(),
                    reason: error.to_string(),
                },
            ))
        })?;

        // WAL for concurrent readers while a writer works; FULL because losing
        // the last records after a crash would lose the ability to say what
        // happened, which is the point of the journal.
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .and_then(|()| connection.pragma_update(None, "synchronous", "FULL"))
            .and_then(|()| connection.pragma_update(None, "foreign_keys", "ON"))
            .map_err(failed)?;

        // Read back, because a pragma that did not take is silent. Every
        // durability claim in this crate rests on `synchronous = FULL`; a
        // filesystem or a build where it quietly stayed at NORMAL would leave
        // the journal promising something it no longer does, and nothing would
        // say so until a crash had already lost the record.
        confirm(&connection, "synchronous", "2")?;
        confirm(&connection, "journal_mode", "wal")?;

        connection.execute_batch(SCHEMA).map_err(failed)?;
        connection
            .execute(
                "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', ?1)",
                params![SCHEMA_VERSION.to_string()],
            )
            .map_err(failed)?;

        Ok(Self { connection })
    }

    /// Records an operation that is planned but has touched nothing.
    ///
    /// Durable when this returns. Everything after it is allowed to assume a
    /// record exists describing what was intended.
    pub fn record_prepared(
        &mut self,
        new: NewOperation,
        transition: &Transition,
    ) -> Result<OperationRecord> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(failed)?;

        let sequence: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(sequence), 0) + 1 FROM operations",
                [],
                |row| row.get(0),
            )
            .map_err(failed)?;

        // Truncated to the resolution the journal actually stores, so the record
        // handed back is the record that will be read later. Returning more
        // precision than survives the round trip is a small lie that would only
        // ever be noticed by something comparing the two.
        let now = now_stored();
        let record = OperationRecord {
            id: OperationId::new(),
            task: new.task,
            plan: new.plan,
            kind: new.kind,
            reverses: new.reverses,
            request: new.request,
            request_digest: new.request_digest,
            sequence: sequence as u64,
            stage: Stage::Prepared,
            transition: transition.clone(),
            payload: new.payload,
            observed: None,
            error_code: None,
            created_at: now,
            updated_at: now,
        };

        transaction
            .execute(
                "INSERT INTO operations
                 (id, task, plan, kind, reverses, request, request_digest, sequence, stage,
                  transition, payload, observed, error_code, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, NULL, ?12, ?12)",
                params![
                    record.id.as_uuid().to_string(),
                    record.task.as_uuid().to_string(),
                    record.plan.as_uuid().to_string(),
                    record.kind.as_str(),
                    record.reverses.map(|id| id.as_uuid().to_string()),
                    record.request.map(|id| id.as_uuid().to_string()),
                    record.request_digest.map(ContentHash::to_hex),
                    sequence,
                    record.stage.as_str(),
                    encode(transition)?,
                    record.payload.map(ContentHash::to_hex),
                    millis(now),
                ],
            )
            .map_err(failed)?;

        transaction.commit().map_err(failed)?;
        Ok(record)
    }

    /// Moves an operation to a new stage, durably.
    pub fn mark(
        &mut self,
        id: OperationId,
        stage: Stage,
        observed: Option<&[PathState]>,
        error_code: Option<&str>,
    ) -> Result<()> {
        let encoded = match observed {
            Some(states) => Some(encode(states)?),
            None => None,
        };
        let changed = self
            .connection
            .execute(
                "UPDATE operations
                 SET stage = ?2, observed = COALESCE(?3, observed),
                     error_code = COALESCE(?4, error_code), updated_at = ?5
                 WHERE id = ?1",
                params![
                    id.as_uuid().to_string(),
                    stage.as_str(),
                    encoded,
                    error_code,
                    millis(SystemTime::now()),
                ],
            )
            .map_err(failed)?;

        if changed == 0 {
            return Err(Error::Denied(Denial::new(
                ErrorCode::PlanNotFound,
                Msg::JournalUnknownOperation {
                    operation: id.to_string(),
                },
            )));
        }
        Ok(())
    }

    /// Looks up an operation.
    pub fn get(&self, id: OperationId) -> Result<OperationRecord> {
        self.query_one("WHERE id = ?1", params![id.as_uuid().to_string()])?
            .ok_or_else(|| {
                Error::Denied(Denial::new(
                    ErrorCode::PlanNotFound,
                    Msg::JournalUnknownOperation {
                        operation: id.to_string(),
                    },
                ))
            })
    }

    /// Resolves a resend of an earlier request.
    ///
    /// - the same key and the same contents: the original record, so the caller
    ///   returns the original result rather than acting twice
    /// - the same key and different contents: a refusal, because guessing which
    ///   was meant could apply a change twice
    /// - an unused key: `None`
    pub fn claim_request(
        &self,
        task: TaskId,
        request: RequestId,
        digest: ContentHash,
    ) -> Result<Option<OperationRecord>> {
        let existing = self.query_one(
            "WHERE task = ?1 AND request = ?2",
            params![task.as_uuid().to_string(), request.as_uuid().to_string()],
        )?;

        match existing {
            Some(record) if record.request_digest == Some(digest) => Ok(Some(record)),
            Some(_) => Err(Error::Denied(Denial::new(
                ErrorCode::RequestMismatch,
                Msg::JournalRequestMismatch {
                    request: request.to_string(),
                },
            ))),
            None => Ok(None),
        }
    }

    /// Operations that have not reached a settled stage.
    ///
    /// What recovery starts from after a crash.
    pub fn unsettled(&self) -> Result<Vec<OperationRecord>> {
        self.query_many(
            "WHERE stage IN ('prepared', 'applying', 'recovery_required') ORDER BY sequence",
            params![],
        )
    }

    /// A task's operations, oldest first.
    pub fn history(&self, task: TaskId) -> Result<Vec<OperationRecord>> {
        self.query_many(
            "WHERE task = ?1 ORDER BY sequence",
            params![task.as_uuid().to_string()],
        )
    }

    /// Every record, oldest first.
    ///
    /// What retention needs: deciding which snapshots are still owed to somebody
    /// is a question about the whole workspace, not about one task.
    pub fn all(&self) -> Result<Vec<OperationRecord>> {
        self.query_many("ORDER BY sequence", params![])
    }

    /// Every state this engine observed after a change it made, oldest first.
    ///
    /// What drift detection needs in order to tell its own work apart from
    /// somebody else's: a file matching one of these was put there by SafeScope,
    /// whatever the baseline says.
    pub fn observed_states(&self) -> Result<Vec<PathState>> {
        Ok(self
            .query_many("WHERE stage = 'committed' ORDER BY sequence", params![])?
            .into_iter()
            .filter_map(|record| record.observed)
            .flatten()
            .collect())
    }

    /// The most recent operation that could be undone.
    ///
    /// Undo walks backwards, so this is the only one eligible: reversing an
    /// earlier operation would have to account for everything done since.
    ///
    /// Reversals and the operations they reversed are both skipped. Without
    /// that, undoing twice would reverse the first undo and put the change back,
    /// walking in a circle rather than backwards.
    pub fn last_undoable(&self, task: TaskId) -> Result<Option<OperationRecord>> {
        self.query_one(
            "WHERE task = ?1
               AND stage = 'committed'
               AND kind = 'change'
               AND id NOT IN (SELECT reverses FROM operations WHERE reverses IS NOT NULL)
             ORDER BY sequence DESC
             LIMIT 1",
            params![task.as_uuid().to_string()],
        )
    }

    fn query_one(
        &self,
        clause: &str,
        parameters: impl rusqlite::Params,
    ) -> Result<Option<OperationRecord>> {
        self.connection
            .prepare(&format!("{SELECT_COLUMNS} {clause}"))
            .map_err(failed)?
            .query_row(parameters, decode_row)
            .optional()
            .map_err(failed)?
            .transpose()
    }

    fn query_many(
        &self,
        clause: &str,
        parameters: impl rusqlite::Params,
    ) -> Result<Vec<OperationRecord>> {
        self.connection
            .prepare(&format!("{SELECT_COLUMNS} {clause}"))
            .map_err(failed)?
            .query_map(parameters, decode_row)
            .map_err(failed)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(failed)?
            .into_iter()
            .collect()
    }
}

const SELECT_COLUMNS: &str = "\
SELECT id, task, plan, kind, reverses, request, request_digest, sequence, stage,
       transition, payload, observed, error_code, created_at, updated_at
FROM operations";

/// Decodes a row, deferring parse failures so the query can report them.
type DecodedRow = rusqlite::Result<Result<OperationRecord>>;

fn decode_row(row: &rusqlite::Row<'_>) -> DecodedRow {
    let get_text = |index: usize| -> rusqlite::Result<String> { row.get(index) };
    let optional_text = |index: usize| -> rusqlite::Result<Option<String>> { row.get(index) };

    Ok((|| -> Result<OperationRecord> {
        Ok(OperationRecord {
            id: parse_id(&get_text(0).map_err(failed)?)?,
            task: parse_id(&get_text(1).map_err(failed)?)?,
            plan: parse_id(&get_text(2).map_err(failed)?)?,
            kind: OperationKind::parse(&get_text(3).map_err(failed)?).ok_or_else(corrupted)?,
            reverses: optional_text(4)
                .map_err(failed)?
                .map(|text| parse_id(&text))
                .transpose()?,
            request: optional_text(5)
                .map_err(failed)?
                .map(|text| parse_id(&text))
                .transpose()?,
            request_digest: optional_text(6)
                .map_err(failed)?
                .map(|text| ContentHash::from_hex(&text))
                .transpose()?,
            sequence: row.get::<_, i64>(7).map_err(failed)? as u64,
            stage: Stage::parse(&get_text(8).map_err(failed)?).ok_or_else(corrupted)?,
            transition: decode(&get_text(9).map_err(failed)?)?,
            payload: optional_text(10)
                .map_err(failed)?
                .map(|text| ContentHash::from_hex(&text))
                .transpose()?,
            observed: optional_text(11)
                .map_err(failed)?
                .map(|text| decode(&text))
                .transpose()?,
            error_code: optional_text(12).map_err(failed)?,
            created_at: from_millis(row.get::<_, i64>(13).map_err(failed)?),
            updated_at: from_millis(row.get::<_, i64>(14).map_err(failed)?),
        })
    })())
}

fn parse_id<T: std::str::FromStr>(text: &str) -> Result<T> {
    text.parse().map_err(|_| corrupted())
}

fn encode<T: serde::Serialize + ?Sized>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|error| {
        Error::Faulted(Fault::new(
            ErrorCode::JournalFailed,
            Msg::JournalOperationFailed {
                reason: error.to_string(),
            },
        ))
    })
}

fn decode<T: serde::de::DeserializeOwned>(text: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|error| {
        Error::Faulted(Fault::new(
            ErrorCode::JournalFailed,
            Msg::JournalOperationFailed {
                reason: error.to_string(),
            },
        ))
    })
}

/// The current time at the journal's own resolution.
fn now_stored() -> SystemTime {
    from_millis(millis(SystemTime::now()))
}

fn millis(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |elapsed| {
        i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
    })
}

fn from_millis(value: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(value.max(0) as u64)
}

/// Fails unless a pragma reads back as the value that was asked for.
fn confirm(connection: &Connection, pragma: &str, expected: &str) -> Result<()> {
    let found: String = connection
        .query_row(&format!("PRAGMA {pragma}"), [], |row| {
            // Both of these come back as either text or an integer depending on
            // the pragma, so whichever it is becomes a string to compare.
            row.get::<_, rusqlite::types::Value>(0)
                .map(|value| match value {
                    rusqlite::types::Value::Text(text) => text,
                    rusqlite::types::Value::Integer(number) => number.to_string(),
                    other => format!("{other:?}"),
                })
        })
        .map_err(failed)?;

    if found.eq_ignore_ascii_case(expected) {
        return Ok(());
    }
    Err(Error::Faulted(
        Fault::new(
            ErrorCode::JournalFailed,
            Msg::JournalPragmaRefused {
                pragma: pragma.to_owned(),
                wanted: expected.to_owned(),
                found,
            },
        )
        .with_hint(Msg::HintJournalNeedsARealFilesystem),
    ))
}

fn failed(error: impl std::fmt::Display) -> Error {
    Error::Faulted(Fault::new(
        ErrorCode::JournalFailed,
        Msg::JournalOperationFailed {
            reason: error.to_string(),
        },
    ))
}

/// A record that cannot be parsed is reported, never guessed at.
fn corrupted() -> Error {
    Error::Faulted(Fault::new(
        ErrorCode::JournalFailed,
        Msg::JournalOperationFailed {
            reason: "a stored record could not be read".to_owned(),
        },
    ))
}
