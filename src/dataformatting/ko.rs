//! Korean messages.

use crate::dataformatting::{Label, Msg};

pub(super) fn render(msg: &Msg) -> String {
    match msg {
        Msg::PathEmpty => "경로가 비어 있습니다.".to_owned(),
        Msg::PathTooLong { len, limit } => {
            format!("경로가 너무 깁니다 ({len} 바이트, 한계 {limit}).")
        }
        Msg::PathAbsolute => {
            "절대 경로는 받지 않습니다. 작업 공간 기준 상대 경로를 쓰세요.".to_owned()
        }
        Msg::PathEmptyComponent { path } => {
            format!("빈 경로 컴포넌트가 있습니다. 앞뒤의 '/' 나 '//' 를 확인하세요: {path:?}")
        }
        Msg::PathRelativeComponent { component } => format!(
            "'{component}' 컴포넌트는 허용하지 않습니다. 작업 공간 기준의 평평한 \
             상대 경로만 받습니다."
        ),
        Msg::PathComponentTooLong { len, limit } => {
            format!("경로 컴포넌트가 너무 깁니다 ({len} 바이트, 한계 {limit}).")
        }
        Msg::PathControlCharacter { codepoint } => {
            format!("경로에 제어 문자가 있습니다 (U+{codepoint:04X}).")
        }
        Msg::PathBackslash => {
            "경로 컴포넌트에 '\\' 를 쓸 수 없습니다. 구분자는 '/' 입니다.".to_owned()
        }
        Msg::PathTrailingDotOrSpace { component } => {
            format!("경로 컴포넌트가 '.' 이나 공백으로 끝날 수 없습니다: {component:?}")
        }
        Msg::PathReservedDeviceName { component } => {
            format!("'{component}' 은 예약된 장치 이름입니다.")
        }
        Msg::PathReservedPrefix { prefix } => {
            format!("'{prefix}' 로 시작하는 이름은 엔진이 예약했습니다.")
        }
        Msg::PathReservedPrefixHint => "다른 파일 이름을 쓰세요.".to_owned(),
        Msg::PathTooDeep { depth, limit } => {
            format!("경로 깊이가 너무 깊습니다 ({depth} 단계, 한계 {limit}).")
        }
        Msg::PatternEmpty => "비어 있는 정책 패턴이 있습니다.".to_owned(),
        Msg::PatternAbsolute { pattern } => {
            format!("정책 패턴 {pattern:?} 이 절대 경로입니다. 패턴은 작업 공간 루트 기준입니다.")
        }
        Msg::PatternTraversal { pattern } => {
            format!("정책 패턴 {pattern:?} 에 '..' 이 있습니다. 허용하지 않습니다.")
        }
        Msg::PatternInvalidGlob { pattern, reason } => {
            format!("정책 패턴 {pattern:?} 이 올바른 glob 이 아닙니다: {reason}")
        }
        Msg::PolicyParseFailed { reason } => {
            format!("정책 파일을 읽지 못했습니다: {reason}")
        }
        Msg::PolicySchemaUnsupported { found, supported } => format!(
            "지원하지 않는 정책 schema_version {found} 입니다. 이 빌드는 {supported} 을 \
             이해합니다."
        ),
        Msg::Label(label) => match label {
            Label::Allowed => "허용",
            Label::Refused => "거부",
            Label::NotCovered => "해당 없음",
            Label::Path => "경로",
            Label::Operation => "연산",
            Label::Policy => "정책",
            Label::PolicyVersion => "버전",
            Label::EvaluationSteps => "평가 과정",
            Label::StepProtected => "보호 경로",
            Label::StepDeny => "deny 규칙",
            Label::StepAllow => "allow 규칙",
            Label::StepGrant => "임시 승인",
            Label::NoMatch => "해당 없음",
            Label::Outcome => "결과",
            Label::ExpansionPossible => "범위 확장을 요청할 수 있습니다",
            Label::ExpansionImpossible => "승인으로 열 수 없습니다",
            Label::CurrentScope => "허용 범위",
            Label::Nothing => "없음",
            Label::Warnings => "경고",
            Label::Approved => "승인됨",
            Label::UnapprovedEdits => "정책 파일에 승인되지 않은 변경이 있습니다",
        }
        .to_owned(),
        Msg::WorkspaceAlreadyRegistered { root } => {
            format!("{root} 은 이미 SafeScope 작업 공간입니다.")
        }
        Msg::WorkspaceNotRegistered { root } => {
            format!("{root} 은 SafeScope 작업 공간이 아닙니다.")
        }
        Msg::WorkspaceIdCorrupted { path } => {
            format!("{path} 의 작업 공간 식별자를 읽을 수 없습니다.")
        }
        Msg::WorkspaceRegistered { root, id } => {
            format!("{root} 을 작업 공간 {id} 로 등록했습니다.")
        }
        Msg::WorkspaceBusyElsewhere { path } => format!(
            "다른 SafeScope 프로세스가 이 작업 공간에 쓰고 있습니다 (잠금은 {path}). \
             예산 검사와 그에 이은 예약 사이에 다른 프로세스가 끼어들지 못하도록 \
             한 번에 하나만 씁니다."
        ),
        Msg::HintAnotherSessionIsWriting => {
            "다른 세션이 끝나기를 기다리거나 그 세션을 닫으세요.".to_owned()
        }
        Msg::HintRunInitFirst => "프로젝트에서 `safescope init` 을 먼저 실행하세요.".to_owned(),
        Msg::HintFillInAllowThenApprove { policy } => format!(
            "아직 아무것도 바꿀 수 없습니다. 허용할 경로를 {policy} 에 적은 뒤 \
             `safescope policy approve` 를 실행하세요."
        ),
        Msg::ExpansionPrompt {
            paths,
            operations,
            reason,
        } => format!(
            "SafeScope 가 바꿀 수 있는 범위를 넓혀달라고 요청하고 있습니다.\n\n\
             경로: {paths}\n\
             연산: {operations}\n\
             제시된 이유: {reason}\n\n\
             승인하면 이 작업에 한해, 제한된 시간 동안, 정확히 이 경로들만 열립니다. \
             정책이 바뀌지는 않습니다."
        ),
        Msg::ExpansionAlreadyAllowed { path } => {
            format!("{path} 은 이미 허용 범위 안입니다. 승인할 것이 없습니다.")
        }
        Msg::ExpansionCannotBeGranted { path } => format!(
            "{path} 은 승인으로 열 수 없습니다. 보호 경로나 deny 규칙은 권한의 문제가 \
             아니어서, 물어봐야 사람의 주의만 낭비합니다."
        ),
        Msg::ExpansionNeedsTerminal => "이 클라이언트는 사람에게 질문할 수 없어서, \
이 경로로는 승인을 받을 수 없습니다."
            .to_owned(),
        Msg::ExpansionLimitReached { used, limit } => format!(
            "이 작업은 클라이언트를 통해 이미 {limit} 건 중 {used} 건의 승인을 받았습니다. \
             이후로는 터미널에서 받아야 합니다. 요청이 길게 이어지다 조용히 넓어지는 일을 \
             막기 위해서입니다."
        ),
        Msg::ExpansionDeclined => "요청이 거절됐습니다. 아무것도 열리지 않았습니다.".to_owned(),
        Msg::ExpansionGranted { paths, minutes } => {
            format!("승인됨: 경로 {paths}개, 이 작업에 한해, {minutes}분 동안.")
        }
        Msg::HintApproveAtATerminal { paths } => {
            format!("터미널에서 `safescope approve {paths}` 를 실행하면 직접 승인할 수 있습니다.")
        }
        Msg::McpInstructions => "SafeScope 는 파일 변경을 승인된 범위와 한도 안에 두고, \
검토하고 되돌릴 수 있도록 기록합니다.\n\n\
prepare_change 로 변경을 검사하고 준비하세요. 이 단계는 아무것도 바꾸지 않습니다. \
그다음 돌려받은 계획 ID 로 apply_change 를 호출하세요. 거부에는 어떤 규칙 때문인지와 \
범위 확장으로 답이 달라질 수 있는지가 담깁니다. 달라질 수 없다고 하면 같은 요청을 \
다시 보내지 마세요.\n\n\
SafeScope 는 이 도구들을 거친 변경만 압니다. 셸 명령이 쓴 것은 기록되지 않고 여기서 \
되돌릴 수 없습니다."
            .to_owned(),
        Msg::McpCoverageNotice => "SafeScope 를 거친 변경만 기록됩니다. 셸 명령이나 다른 \
도구가 쓴 파일은 여기서 다루지 않으며 되돌릴 수 없습니다."
            .to_owned(),
        Msg::McpMissingField { field } => {
            format!("이 연산에는 {field} 이(가) 필요한데 주어지지 않았습니다.")
        }
        Msg::McpUnknownPlan { plan } => format!(
            "준비된 계획 {plan} 이 없습니다. 이미 실행됐거나, 그 뒤에 서버가 다시 \
             시작됐을 수 있습니다."
        ),
        Msg::McpTransportFailed { reason } => {
            format!("MCP 연결을 제공하지 못했습니다: {reason}")
        }
        Msg::HookUnsettledWork { count } => format!(
            "SafeScope 에 기록을 마치지 못한 연산이 {count}건 있습니다. 디스크의 상태를 \
             믿기 전에 `safescope recover` 를 실행하세요."
        ),
        Msg::HookNeedsAttention { count } => format!(
            "SafeScope 가 연산 {count}건의 결과를 말할 수 없습니다. 사람이 대조해야 합니다. \
             자동으로 고친 것은 없습니다."
        ),
        Msg::HookPolicyEdited => "SafeScope 정책 파일이 수정됐지만 승인되지 않아 적용되지 \
않았습니다. 의도한 변경이라면 `safescope policy approve` 를 실행하세요."
            .to_owned(),
        Msg::UndoNothingRecorded => "되돌릴 것이 없습니다. 이 작업에 완료된 연산이 \
없습니다. 결과가 아직 불확실한 연산은 먼저 확정해야 합니다."
            .to_owned(),
        Msg::UndoConflictAt { path } => format!(
            "{path} 이 SafeScope 가 마지막으로 건드린 뒤 바뀌었습니다. 되돌리면 그 뒤에 \
             들어온 내용을 덮어씁니다. 아무것도 하지 않았습니다."
        ),
        Msg::HintCompareBeforeUndoing { path } => format!(
            "{path} 을 보관된 이전 내용과 비교해서 무엇을 남길지 정하세요. 복구 자료는 \
             어느 쪽이든 그대로 보관됩니다."
        ),
        Msg::BudgetPathsExceeded {
            used,
            limit,
            adding,
        } => format!(
            "이 작업은 허용된 {limit} 개 경로 중 이미 {used} 개를 바꿨고, {adding} 은 그 \
             다음이 됩니다. 아무것도 하지 않았습니다."
        ),
        Msg::BudgetOperationsExceeded { used, limit } => {
            format!("이 작업은 허용된 {limit} 회 중 {used} 회를 썼습니다.")
        }
        Msg::BudgetMovesExceeded { used, limit } => {
            format!("이 작업은 허용된 이동 {limit} 회 중 {used} 회를 썼습니다.")
        }
        Msg::BudgetStorageExceeded { used, limit } => format!(
            "이 작업 공간의 복구 자료가 한계 {limit} 에 대해 {used} 입니다. 이 변경이 \
             없앨 내용을 보관할 수 없습니다."
        ),
        Msg::HintRequestBudgetExpansion => "지금까지 바꾼 것을 검토하세요. 정말 더 필요하면 \
다시 시도하지 말고 한도 확장을 요청하세요."
            .to_owned(),
        Msg::PlanTargetExists { path } => format!(
            "{path} 이 이미 있어서 생성할 수 없습니다. 교체는 다른 연산이고, 교체는 이전 \
             내용을 먼저 저장합니다."
        ),
        Msg::PlanTargetMissing { path } => {
            format!("{path} 이 없어서 이 연산이 대상으로 삼을 것이 없습니다.")
        }
        Msg::PlanFileTooLarge { path, size, limit } => {
            format!("{path} 은 {size} 로, 이 정책이 허용하는 {limit} 를 넘습니다.")
        }
        Msg::PlanHasExpired { plan } => {
            format!("계획 {plan} 의 유효기간이 지나 실행하지 않습니다.")
        }
        Msg::PlanStateChanged { path } => format!(
            "{path} 이 계획을 세운 뒤 바뀌었습니다. 아무것도 하지 않았습니다. 다른 내용을 \
             기준으로 만든 계획을 적용하면 그 사이에 들어온 변경을 덮어씁니다."
        ),
        Msg::HintRebuildThePlan => "지금 내용을 기준으로 계획을 다시 만드세요.".to_owned(),
        Msg::JournalOpenFailed { path, reason } => {
            format!("{path} 의 작업 기록을 열지 못했습니다: {reason}")
        }
        Msg::JournalOperationFailed { reason } => {
            format!("작업 기록을 쓰지 못했습니다: {reason}")
        }
        Msg::JournalRequestMismatch { request } => format!(
            "요청 {request} 이 다른 내용으로 이미 기록돼 있습니다. 어느 쪽을 뜻했는지 \
             추측하면 같은 변경을 두 번 적용할 수 있어서, 새 요청으로 보지 않고 보고합니다."
        ),
        Msg::JournalUnknownOperation { operation } => {
            format!("연산 {operation} 에 대한 작업 기록이 없습니다.")
        }
        Msg::SnapshotVerificationFailed { hash } => format!(
            "{hash} 의 복구 자료를 다시 해시한 값이 저장한 값과 다릅니다. 아무것도 바꾸지 \
             않았습니다. 다시 읽어서 맞는 복구 자료가 없으면 변경을 되돌릴 수 없습니다."
        ),
        Msg::SnapshotMissing { hash } => {
            format!("{hash} 의 복구 자료가 저장소에 없습니다.")
        }
        Msg::SnapshotStoreFailed { reason } => {
            format!("복구 자료를 저장하지 못했습니다: {reason}. 원본은 그대로 두었습니다.")
        }
        Msg::StoreDataDirectoryUnavailable => "엔진 상태를 둘 디렉터리를 찾을 수 없습니다. \
SAFESCOPE_DATA_DIR 로 지정하세요."
            .to_owned(),
        Msg::StoreWriteFailed { path, reason } => {
            format!("{path} 에 엔진 상태를 쓰지 못했습니다: {reason}")
        }
        Msg::StoreReadFailed { path, reason } => {
            format!("{path} 에서 엔진 상태를 읽지 못했습니다: {reason}")
        }
        Msg::StoreCorrupted { path, reason } => format!(
            "{path} 의 엔진 상태를 읽을 수 없습니다: {reason}. 자동으로 고치지 않았습니다. \
             손상된 상태를 추측으로 복구하는 것이 멀쩡한 데이터를 잃는 경로입니다."
        ),
        Msg::StoreStateInsideWorkspace { state, workspace } => format!(
            "엔진 상태가 작업 공간 {workspace} 안의 {state} 에 놓이게 됩니다. 그러면 복구 \
             자료가 자기가 보호하는 트리 안에 들어가므로 등록을 거부합니다."
        ),
        Msg::PathDestinationExists { path } => format!(
            "{path} 이 이미 있습니다. 이동은 도착지를 덮어쓰지 않습니다. 사라질 파일의 \
             복구 자료가 없기 때문입니다."
        ),
        Msg::PlatformCrossFilesystem { from, to } => format!(
            "{from} 과 {to} 가 서로 다른 파일시스템에 있습니다. 그 사이의 이동은 원자적일 수 \
             없어서, 복사 후 삭제로 대신하지 않고 거부합니다."
        ),
        Msg::PlatformAtomicRenameUnsupported { reason } => format!(
            "이 파일시스템은 덮어쓰지 않는 이름 변경을 지원하지 않습니다 ({reason}). 먼저 \
             확인하고 나중에 이름을 바꾸면 이 연산이 막으려던 경합이 되살아나므로, \
             대신 거부합니다."
        ),
        Msg::PlatformOperationFailed { operation, reason } => {
            format!("{operation} 연산이 실패했습니다: {reason}")
        }
        Msg::WorkspaceOpenFailed { root, reason } => {
            format!("{root} 의 작업 공간을 열지 못했습니다: {reason}")
        }
        Msg::PathNotARegularFile { path } => {
            format!("{path} 은 일반 파일이 아닙니다. 이 버전은 일반 파일 하나씩만 다룹니다.")
        }
        Msg::PathComponentNotADirectory { path, component } => {
            format!("{path} 을 해석하는 중 {component:?} 이 디렉터리가 아닙니다.")
        }
        Msg::PathSymlinkRefused { path } => format!(
            "{path} 은 심볼릭 링크입니다. 따라가면 검사한 경로 밖에서 작업하게 되므로 \
             해석하지 않고 거부합니다."
        ),
        Msg::PathParentMissing { path, parent } => {
            format!("{parent} 디렉터리가 없어서 {path} 을 만들 수 없습니다.")
        }
        Msg::HintCreateTheDirectoryFirst => "이 버전은 디렉터리를 만들지 않습니다. 직접 만든 \
뒤에 다시 시도하세요."
            .to_owned(),
        Msg::ScopeNotCovered { path } => {
            format!("{path} 은 허용 범위의 어떤 규칙에도 해당하지 않습니다.")
        }
        Msg::ScopeDeniedByRule { path, pattern } => {
            format!("{path} 은 deny 규칙 {pattern:?} 에 의해 거부됐습니다.")
        }
        Msg::ScopeOperationNotAllowed {
            path,
            operation,
            allowed,
        } => format!(
            "{path} 은 바꿀 수 있지만 {operation} 은 허용되지 않습니다. 여기서 허용된 연산: \
             {allowed}."
        ),
        Msg::HintExpansionMayBeRequested => "이 경로가 꼭 필요하다면 같은 편집을 다시 \
시도하지 말고 safescope 로 범위 확장을 요청하세요."
            .to_owned(),
        Msg::HintPolicyDenyIsFinal => "deny 규칙은 승인으로 풀 수 없습니다. 잘못됐다면 정책 \
파일을 고치고 승인하세요."
            .to_owned(),
        Msg::PolicyNoAllowRules => "정책에 allow 규칙이 없어서 아무것도 바꿀 수 없습니다. \
의도한 것이라면 빈 목록으로 두지 말고 명시하세요."
            .to_owned(),
        Msg::PolicyEmptyDefaultOps => "default_ops 가 비어 있어서 간단형 allow 규칙이 \
아무 연산도 허용하지 않게 됩니다."
            .to_owned(),
        Msg::PolicyRuleGrantsNothing { pattern } => {
            format!("{pattern:?} 규칙이 아무 연산도 허용하지 않습니다.")
        }
        Msg::PolicyAllowOverProtected { pattern, protected } => format!(
            "allow 규칙 {pattern:?} 이 엔진이 항상 보호하는 {protected:?} 에 닿습니다. \
             이 규칙은 효과가 없습니다."
        ),
        Msg::PolicyWorkspaceWideNeedsOptIn { pattern } => format!(
            "allow 규칙 {pattern:?} 이 작업 공간 전체를 덮습니다. 의도한 것이라면 \
             safety.unsafe_allow_workspace_wide = true 를 설정하세요."
        ),
        Msg::PolicyAllowAlsoDenied { pattern } => format!(
            "{pattern:?} 이 allow 와 deny 양쪽에 있습니다. deny 가 항상 이기므로 의도가 \
             불분명합니다."
        ),
        Msg::PolicyDuplicatePattern { pattern } => {
            format!("패턴 {pattern:?} 이 여러 번 나옵니다.")
        }
        Msg::PolicyDenyNeverApplies { pattern } => {
            format!("deny 규칙 {pattern:?} 이 어떤 allow 규칙과도 겹치지 않아 효과가 없습니다.")
        }
        Msg::PolicyBudgetZero { field } => {
            format!("budget.{field} 이 0 이라서 모든 연산이 거부됩니다.")
        }
        Msg::PolicyFileLimitExceedsSnapshotLimit {
            file_bytes,
            snapshot_bytes,
        } => format!(
            "max_file_bytes ({file_bytes}) 이 max_snapshot_bytes ({snapshot_bytes}) 보다 \
             큽니다. 허용된 파일의 복구 자료를 저장할 수 없게 됩니다."
        ),
        Msg::PolicyWarnRatioOutOfRange { value } => {
            format!("budget.warn_at_ratio 가 {value} 입니다. 0 과 1 사이여야 합니다.")
        }
        Msg::ProtectedEngineState => "엔진 자신의 정책과 작업 공간 상태입니다. \
이걸 바꿀 수 있으면 엔진이 자기 판단 근거를 고쳐 쓸 수 있습니다."
            .to_owned(),
        Msg::ProtectedGitHistory => "Git 히스토리입니다. 복구 판정이 히스토리가 온전하다는 \
전제 위에 서 있어서, 여길 고칠 수 있으면 충돌 판정의 의미가 사라집니다."
            .to_owned(),
        Msg::ProtectedPermissionSurface => "Claude Code 의 권한 설정입니다. 여기에 쓸 수 \
있으면 도구 제한을 안에서 풀 수 있습니다."
            .to_owned(),
        Msg::ProtectedTemporaryName => "엔진의 원자적 교체가 쓰려고 예약한 이름입니다.".to_owned(),
        Msg::HashBadLength { len } => {
            format!("해시 길이가 잘못됐습니다 ({len} 글자, 64 글자여야 합니다).")
        }
        Msg::HashNotHexadecimal { text } => {
            format!("해시에 16진수가 아닌 값이 있습니다: {text:?}")
        }
        Msg::FaultUnknownValue { variable, value } => {
            format!("알 수 없는 {variable} 값입니다: {value:?}")
        }
        Msg::FaultAborting { point } => {
            format!("주입된 크래시 지점 {point} 에서 중단합니다.")
        }
        Msg::CliNotImplemented { version } => {
            format!("safescope {version} — 명령줄은 아직 없습니다 (M1 예정).")
        }
    }
}
