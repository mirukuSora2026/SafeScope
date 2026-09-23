//! Japanese messages.

use crate::dataformatting::Msg;

mod labels;

pub(super) fn render(msg: &Msg) -> String {
    match msg {
        Msg::PathEmpty => "パスが空です。".to_owned(),
        Msg::PathTooLong { len, limit } => {
            format!("パスが長すぎます（{len} バイト、上限は {limit}）。")
        }
        Msg::PathAbsolute => {
            "絶対パスは受け付けません。ワークスペースルートからの相対パスを使ってください。"
                .to_owned()
        }
        Msg::PathEmptyComponent { path } => {
            format!(
                "パスに空の要素があります。先頭・末尾・重複した '/' を確認してください: {path:?}"
            )
        }
        Msg::PathRelativeComponent { component } => {
            format!(
                "'{component}' という要素は使えません。ワークスペースルートからの\
                 平坦な相対パスのみ受け付けます。"
            )
        }
        Msg::PathComponentTooLong { len, limit } => {
            format!("パスの要素が長すぎます（{len} バイト、上限は {limit}）。")
        }
        Msg::PathControlCharacter { codepoint } => {
            format!("パスに制御文字が含まれています（U+{codepoint:04X}）。")
        }
        Msg::PathBackslash => "パスの要素に '\\' は使えません。区切り文字は '/' です。".to_owned(),
        Msg::PathTrailingDotOrSpace { component } => {
            format!("パスの要素を '.' や空白で終えることはできません: {component:?}")
        }
        Msg::PathReservedDeviceName { component } => {
            format!("'{component}' は予約されたデバイス名です。")
        }
        Msg::PathReservedPrefix { prefix } => {
            format!("'{prefix}' で始まる名前はエンジンが予約しています。")
        }
        Msg::PathReservedPrefixHint => "別のファイル名を使ってください。".to_owned(),
        Msg::PathTooDeep { depth, limit } => {
            format!("パスの階層が深すぎます（{depth} 段、上限は {limit}）。")
        }
        Msg::PatternEmpty => "空のポリシーパターンがあります。".to_owned(),
        Msg::PatternAbsolute { pattern } => format!(
            "ポリシーパターン {pattern:?} は絶対パスです。パターンはワークスペース\
             ルートからの相対指定です。"
        ),
        Msg::PatternTraversal { pattern } => {
            format!("ポリシーパターン {pattern:?} に '..' が含まれています。許可されていません。")
        }
        Msg::PatternInvalidGlob { pattern, reason } => {
            format!("ポリシーパターン {pattern:?} は有効な glob ではありません: {reason}")
        }
        Msg::PolicyParseFailed { reason } => {
            format!("ポリシーファイルを読み込めませんでした: {reason}")
        }
        Msg::PolicySchemaUnsupported { found, supported } => format!(
            "サポートされていないポリシーの schema_version {found} です。\
             このビルドが理解するのは {supported} です。"
        ),
        Msg::Label(label) => labels::render(*label).to_owned(),
        Msg::WorkspaceAlreadyRegistered { root } => {
            format!("{root} はすでに SafeScope のワークスペースです。")
        }
        Msg::WorkspaceNotRegistered { root } => {
            format!("{root} は SafeScope のワークスペースではありません。")
        }
        Msg::WorkspaceIdCorrupted { path } => {
            format!("{path} のワークスペース識別子を読み取れません。")
        }
        Msg::WorkspaceRegistered { root, id } => {
            format!("{root} をワークスペース {id} として登録しました。")
        }
        Msg::WorkspaceBusyElsewhere { path } => format!(
            "別の SafeScope プロセスがこのワークスペースに書き込んでいます（ロックは {path}）。\
             予算の確認とそれに続く予約の間に他が割り込まないよう、書き込みは一度に一つです。"
        ),
        Msg::HintAnotherSessionIsWriting => {
            "他のセッションが終わるのを待つか、閉じてください。".to_owned()
        }
        Msg::HintRunInitFirst => {
            "先にプロジェクトで `safescope init` を実行してください。".to_owned()
        }
        Msg::HintFillInAllowThenApprove { policy } => format!(
            "まだ何も変更できません。許可したいパスを {policy} に書いてから \
             `safescope policy approve` を実行してください。"
        ),
        Msg::ExpansionPrompt {
            paths,
            operations,
            reason,
        } => format!(
            "SafeScope が変更できる範囲の拡大を求めています。\n\n\
             パス: {paths}\n\
             操作: {operations}\n\
             提示された理由: {reason}\n\n\
             承認すると、このタスクに限り、限られた時間だけ、ここに挙げたパスだけが\
             開きます。ポリシーが変わるわけではありません。"
        ),
        Msg::ExpansionAlreadyAllowed { path } => {
            format!("{path} はすでに許可範囲内です。承認するものはありません。")
        }
        Msg::ExpansionCannotBeGranted { path } => format!(
            "{path} は承認では開けません。保護パスや deny ルールは権限の問題ではないため、\
             尋ねても人の注意を無駄にするだけです。"
        ),
        Msg::ExpansionNeedsTerminal => "このクライアントは人に問いかけられないため、\
この経路では承認を得られません。"
            .to_owned(),
        Msg::ExpansionLimitReached { used, limit } => format!(
            "このタスクはクライアント経由ですでに {limit} 件中 {used} 件の承認を得ています。\
             以降は端末で行う必要があります。要求が続くうちに静かに範囲が広がることを\
             防ぐためです。"
        ),
        Msg::ExpansionDeclined => "要求は拒否されました。何も開かれていません。".to_owned(),
        Msg::ExpansionGranted { paths, minutes } => {
            format!("承認しました: パス {paths} 件、このタスクに限り、{minutes} 分間。")
        }
        Msg::HintApproveAtATerminal { paths } => {
            format!("端末で `safescope approve {paths}` を実行すれば自分で承認できます。")
        }
        Msg::McpInstructions => "SafeScope はファイル変更を承認済みの範囲と上限の内側に\
とどめ、確認と取り消しができるよう記録します。\n\n\
まず prepare_change で変更を検査し準備します。この段階では何も変更されません。\
次に返された計画 ID で apply_change を呼びます。拒否にはどのルールによるものかと、\
範囲の拡張で結果が変わりうるかが示されます。変わらないとある場合は、\
同じ要求を繰り返さないでください。\n\n\
SafeScope が把握しているのはこれらのツールを通した変更だけです。\
シェルコマンドが書いたものは記録されず、ここでは取り消せません。"
            .to_owned(),
        Msg::McpCoverageNotice => "SafeScope を通した変更のみが記録されます。\
シェルコマンドや他のツールが書いたファイルは対象外で、ここでは取り消せません。"
            .to_owned(),
        Msg::McpMissingField { field } => {
            format!("この操作には {field} が必要ですが、与えられていません。")
        }
        Msg::McpUnknownPlan { plan } => format!(
            "準備された計画 {plan} はありません。すでに実行されたか、\
             その後サーバーが再起動した可能性があります。"
        ),
        Msg::McpTransportFailed { reason } => {
            format!("MCP 接続を提供できませんでした: {reason}")
        }
        Msg::HookUnsettledWork { count } => format!(
            "SafeScope に記録を終えていない操作が {count} 件あります。ディスク上の状態を\
             当てにする前に `safescope recover` を実行してください。"
        ),
        Msg::HookNeedsAttention { count } => format!(
            "SafeScope は {count} 件の操作の結果を判定できません。人が照合する必要があります。\
             自動的に修復したものはありません。"
        ),
        Msg::HookPolicyEdited => "SafeScope のポリシーファイルは編集されていますが承認\
されていないため、変更は有効になっていません。意図した変更であれば \
`safescope policy approve` を実行してください。"
            .to_owned(),
        Msg::HookChangedOutside { count } => format!(
            "{count} 個のファイルが SafeScope を経由せずに変更されました。スナップショットが \
             ないため元に戻せません。`safescope status` でどのファイルか確認してください。"
        ),
        Msg::HookToolNotAllowed { tool } => format!(
            "承認済みポリシーにより SafeScope は許可リストモードで動作しており、`{tool}` は \
             リストにありません。変更が記録されるよう SafeScope 経由でファイルを変更してください。"
        ),
        Msg::DriftNoBaseline => {
            "基準となる状態が記録されていないため、外部で何が変更されたかを示せません。".to_owned()
        }
        Msg::DriftClean { scanned } => {
            format!("SafeScope の外で変更されたものはありません（{scanned} 個のファイルを確認）。")
        }
        Msg::DriftUnrecoverable { count } => {
            format!("このうち {count} 個は以前の内容が保存されておらず、元に戻せません。")
        }
        Msg::DriftTruncated { scanned } => format!(
            "先頭の {scanned} 個のファイルのみ確認したため、この一覧は不完全な可能性があります。"
        ),
        Msg::HintReviewDrift => {
            "確認のうえ、`safescope drift accept` で現在の状態を新しい基準にしてください。"
                .to_owned()
        }
        Msg::HintAllowlistMode { allowed } => format!("ポリシーが許可するツール: {allowed}"),
        Msg::GuardUnsupportedHere => "保護付きの実行にはカーネルサンドボックスが必要ですが、このプラットフォームにはありません。"
            .to_owned(),
        Msg::HintGuardNeedsSandbox => "`safescope guard` は macOS で sandbox-exec を用いて動作します。それ以外では \
コマンドをそのまま実行し、`safescope drift` でエンジンの外で何が変わったかを確認してください。"
            .to_owned(),
        Msg::GuardStarting { path } => format!(
            "{path} を保護します。このコマンドが起動するものは一切そこへ書き込めず、\
             変更は SafeScope を経由する必要があります。"
        ),
        Msg::DurabilityLimitedHere => "このプラットフォームはディレクトリをフラッシュできないため、変更直後のクラッシュで\
その変更が失われることがあります。書きかけのファイルが残ることはなく、見つかったものは \
`safescope recover` が報告します。"
            .to_owned(),
        Msg::PolicyAllowToolsWithoutAllowlist => "`allow_tools` が設定されていますが、モードは `audit` です。audit では名前による拒否を\
行わないため、この一覧は効果を持ちません。意図したものであれば `mode = \"allowlist\"` を設定してください。"
            .to_owned(),
        Msg::PolicyAllowToolsReopensTheGap { tool } => format!(
            "`allow_tools` に `{tool}` が含まれています。シェルコマンドを実行できるツールのため、\
             許可するとこのモードが閉じようとしていた経路が再び開きます。その経路での変更は記録されません。"
        ),
        Msg::HintNothingToUndo => "このタスクには元に戻せる記録がありません。取り消しは一度に一操作ずつ、SafeScope を\
経由した変更に対してのみ行えます。エンジンの外で行われた変更は、戻すための内容が保存されていません。"
            .to_owned(),
        Msg::HintWhatTheOperationNeeds { operation, field } => format!(
            "`{operation}` には `{field}` が必要です。その名前のとおり正確に送ってください — \
             似た名前は別の綴りではなく、欠けたフィールドとして扱われます。"
        ),
        Msg::HintSnapshotMayHaveAged => "復旧データはタスク終了後 `retain_closed_task_days` の間保持され、これはそれより\
古い可能性があります。何が起きたかの履歴はそのまま残っており、失われたのは元に戻すための内容だけです。"
            .to_owned(),
        Msg::JournalPragmaRefused { pragma, wanted, found } => format!(
            "ジャーナルを `{pragma} = {wanted}` に設定できませんでした。現在は `{found}` です。\
             クラッシュ後も残るという主張はすべてこの設定に依存するため、守れない約束をするくらいなら\
             ジャーナルを開きません。"
        ),
        Msg::HintJournalNeedsARealFilesystem => "多くの場合、状態ディレクトリがその設定に対応していないファイルシステム上にあります — \
ネットワーク共有や一部のコンテナマウントです。SAFESCOPE_DATA_DIR をローカルストレージに向けてください。"
            .to_owned(),
        Msg::RecoveryFoundNothing => "復旧が必要なものはありませんでした。".to_owned(),
        Msg::RecoverySettled { aborted, committed } => {
            format!("実行された操作 {committed} 件と、されなかった {aborted} 件を確定しました。")
        }
        Msg::RecoveryLeftUnresolved { count } => format!(
            "{count} 件の操作を確定できませんでした。ディスク上の内容が、以前の状態とも\
             変更後になるはずの状態とも一致しません。何も修復していません。\
             ここで推測することこそ、無事な作業を壊す道です。"
        ),
        Msg::RecoveryRemovedTemporaries { count } => {
            format!("クラッシュが残した一時ファイル {count} 件を削除しました。")
        }
        Msg::DoctorHealthy => "ここで点検した項目はすべて問題ありません。".to_owned(),
        Msg::UndoNothingRecorded => "元に戻すものがありません。このタスクには完了した\
操作がありません。結果がまだ不明な操作は先に確定する必要があります。"
            .to_owned(),
        Msg::UndoConflictAt { path } => format!(
            "{path} は SafeScope が最後に触れたあとに変更されています。元に戻すと\
             その後に入った内容を上書きしてしまいます。何もしていません。"
        ),
        Msg::HintCompareBeforeUndoing { path } => format!(
            "{path} を保存されている以前の内容と比較して、どちらを残すか決めてください。\
             復旧データはいずれにせよ保管されています。"
        ),
        Msg::BudgetPathsExceeded {
            used,
            limit,
            adding,
        } => format!(
            "このタスクは許可された {limit} 個のパスのうちすでに {used} 個を変更しており、\
             {adding} はさらにもう一つになります。何もしていません。"
        ),
        Msg::BudgetOperationsExceeded { used, limit } => {
            format!("このタスクは許可された {limit} 回のうち {used} 回を使いました。")
        }
        Msg::BudgetMovesExceeded { used, limit } => {
            format!("このタスクは許可された移動 {limit} 回のうち {used} 回を使いました。")
        }
        Msg::BudgetStorageExceeded { used, limit } => format!(
            "このワークスペースの復旧データは上限 {limit} に対して {used} です。\
             この変更が失わせる内容を保管できません。"
        ),
        Msg::HintRequestBudgetExpansion => "これまでの変更を確認してください。\
本当に必要であれば、やり直すのではなく上限の引き上げを要求してください。"
            .to_owned(),
        Msg::PlanTargetExists { path } => format!(
            "{path} はすでに存在するため作成できません。置換は別の操作であり、\
             置換は以前の内容を先に保存します。"
        ),
        Msg::PlanTargetMissing { path } => {
            format!("{path} が存在しないため、この操作の対象がありません。")
        }
        Msg::PlanFileTooLarge { path, size, limit } => {
            format!("{path} は {size} で、このポリシーが許可する {limit} を超えています。")
        }
        Msg::PlanHasExpired { plan } => {
            format!("計画 {plan} は期限切れのため実行しません。")
        }
        Msg::PlanStateChanged { path } => format!(
            "{path} は計画の作成後に変更されました。何もしていません。異なる内容を前提に\
             作られた計画を適用すると、その間に入った変更を上書きしてしまいます。"
        ),
        Msg::HintRebuildThePlan => "現在の内容を前提に計画を作り直してください。".to_owned(),
        Msg::JournalOpenFailed { path, reason } => {
            format!("{path} の作業記録を開けませんでした: {reason}")
        }
        Msg::JournalOperationFailed { reason } => {
            format!("作業記録を書き込めませんでした: {reason}")
        }
        Msg::JournalRequestMismatch { request } => format!(
            "リクエスト {request} は別の内容ですでに記録されています。どちらの意図かを\
             推測すると同じ変更を二度適用しかねないため、新しいリクエストとは見なさず\
             報告します。"
        ),
        Msg::JournalUnknownOperation { operation } => {
            format!("操作 {operation} に対応する作業記録がありません。")
        }
        Msg::SnapshotVerificationFailed { hash } => format!(
            "{hash} の復旧データを再ハッシュした値が保存した値と一致しません。\
             何も変更していません。正しく読み戻せる復旧データがなければ、\
             その変更は元に戻せないからです。"
        ),
        Msg::SnapshotMissing { hash } => {
            format!("{hash} の復旧データがストアにありません。")
        }
        Msg::SnapshotStoreFailed { reason } => {
            format!("復旧データを保存できませんでした: {reason}。原本はそのままです。")
        }
        Msg::StoreDataDirectoryUnavailable => "エンジン状態を置くディレクトリが見つかりません。\
SAFESCOPE_DATA_DIR で指定してください。"
            .to_owned(),
        Msg::StoreWriteFailed { path, reason } => {
            format!("{path} にエンジン状態を書き込めませんでした: {reason}")
        }
        Msg::StoreReadFailed { path, reason } => {
            format!("{path} からエンジン状態を読み込めませんでした: {reason}")
        }
        Msg::StoreCorrupted { path, reason } => format!(
            "{path} のエンジン状態を読み取れません: {reason}。自動的には修復していません。\
             壊れた状態を推測で直すことこそ、無事なデータを失う道だからです。"
        ),
        Msg::StoreStateInsideWorkspace { state, workspace } => format!(
            "エンジン状態がワークスペース {workspace} の内側の {state} に置かれます。\
             復旧データが保護対象のツリー内に入るため、登録を拒否します。"
        ),
        Msg::PathDestinationExists { path } => format!(
            "{path} はすでに存在します。移動は宛先を上書きしません。失われるファイルの\
             復旧データがないからです。"
        ),
        Msg::PlatformCrossFilesystem { from, to } => format!(
            "{from} と {to} は別のファイルシステム上にあります。その間の移動は\
             アトミックにできないため、コピーと削除で代替せず拒否します。"
        ),
        Msg::PlatformAtomicRenameUnsupported { reason } => format!(
            "このファイルシステムは上書きしないリネームに対応していません（{reason}）。\
             先に確認してから名前を変えると、この操作が閉じようとしている競合が\
             よみがえるため、代わりに拒否します。"
        ),
        Msg::PlatformOperationFailed { operation, reason } => {
            format!("{operation} 操作が失敗しました: {reason}")
        }
        Msg::WorkspaceOpenFailed { root, reason } => {
            format!("{root} のワークスペースを開けませんでした: {reason}")
        }
        Msg::PathNotARegularFile { path } => {
            format!(
                "{path} は通常ファイルではありません。このバージョンは通常ファイルのみ扱います。"
            )
        }
        Msg::PathComponentNotADirectory { path, component } => {
            format!("{path} の解決中に {component:?} がディレクトリではありませんでした。")
        }
        Msg::PathSymlinkRefused { path } => format!(
            "{path} はシンボリックリンクです。たどると検査したパスの外で操作することに\
             なるため、解決せずに拒否します。"
        ),
        Msg::PathParentMissing { path, parent } => {
            format!("ディレクトリ {parent} が存在しないため {path} を作成できません。")
        }
        Msg::HintCreateTheDirectoryFirst => "このバージョンはディレクトリを作成しません。\
自分で作成してから再試行してください。"
            .to_owned(),
        Msg::ScopeNotCovered { path } => {
            format!("{path} は許可範囲のどのルールにも一致しません。")
        }
        Msg::ScopeDeniedByRule { path, pattern } => {
            format!("{path} は deny ルール {pattern:?} により拒否されました。")
        }
        Msg::ScopeOperationNotAllowed {
            path,
            operation,
            allowed,
        } => format!(
            "{path} は変更できますが、{operation} は許可されていません。\
             ここで許可されている操作: {allowed}。"
        ),
        Msg::HintExpansionMayBeRequested => "このパスが本当に必要であれば、同じ編集を\
やり直すのではなく safescope で範囲の拡張を要求してください。"
            .to_owned(),
        Msg::HintPolicyDenyIsFinal => "deny ルールは承認では解除できません。誤りであれば\
ポリシーファイルを修正して承認してください。"
            .to_owned(),
        Msg::PolicyNoAllowRules => "ポリシーに allow ルールがないため、何も変更できません。\
意図的であれば、空のリストのままにせず明示してください。"
            .to_owned(),
        Msg::PolicyEmptyDefaultOps => "default_ops が空のため、簡易形式の allow ルールが\
どの操作も許可しなくなります。"
            .to_owned(),
        Msg::PolicyRuleGrantsNothing { pattern } => {
            format!("ルール {pattern:?} はどの操作も許可していません。")
        }
        Msg::PolicyAllowOverProtected { pattern, protected } => format!(
            "allow ルール {pattern:?} はエンジンが常に保護する {protected:?} に届きます。\
             このルールは効果を持ちません。"
        ),
        Msg::PolicyWorkspaceWideNeedsOptIn { pattern } => format!(
            "allow ルール {pattern:?} はワークスペース全体を覆います。意図的であれば \
             safety.unsafe_allow_workspace_wide = true を設定してください。"
        ),
        Msg::PolicyAllowAlsoDenied { pattern } => format!(
            "{pattern:?} が allow と deny の両方にあります。deny が常に優先するため、\
             意図が不明です。"
        ),
        Msg::PolicyDuplicatePattern { pattern } => {
            format!("パターン {pattern:?} が複数回書かれています。")
        }
        Msg::PolicyDenyNeverApplies { pattern } => format!(
            "deny ルール {pattern:?} はどの allow ルールとも重ならないため、効果がありません。"
        ),
        Msg::PolicyBudgetZero { field } => {
            format!("budget.{field} が 0 のため、すべての操作が拒否されます。")
        }
        Msg::PolicyFileLimitExceedsSnapshotLimit {
            file_bytes,
            snapshot_bytes,
        } => format!(
            "max_file_bytes（{file_bytes}）が max_snapshot_bytes（{snapshot_bytes}）を\
             超えており、許可されたファイルの復旧データを保存できません。"
        ),
        Msg::PolicyWarnRatioOutOfRange { value } => {
            format!("budget.warn_at_ratio が {value} です。0 と 1 の間である必要があります。")
        }
        Msg::ProtectedEngineState => "エンジン自身のポリシーとワークスペース状態です。\
変更できると、エンジンが自らの判断根拠を書き換えられてしまいます。"
            .to_owned(),
        Msg::ProtectedGitHistory => "Git の履歴です。復旧の判定は履歴が無傷であることを\
前提としており、書き換え可能だと衝突判定の意味が失われます。"
            .to_owned(),
        Msg::ProtectedPermissionSurface => "Claude Code の権限設定です。書き込めると、\
ツール制限を内側から解除できてしまいます。"
            .to_owned(),
        Msg::ProtectedTemporaryName => {
            "エンジンのアトミック置換が予約している名前です。".to_owned()
        }
        Msg::HashBadLength { len } => {
            format!("ハッシュの長さが不正です（{len} 文字、64 文字である必要があります）。")
        }
        Msg::HashNotHexadecimal { text } => {
            format!("ハッシュに 16 進数でない値が含まれています: {text:?}")
        }
        Msg::FaultUnknownValue { variable, value } => {
            format!("認識できない {variable} の値です: {value:?}")
        }
        Msg::FaultAborting { point } => {
            format!("注入されたフォールトポイント {point} で中断します。")
        }
        Msg::CliNotImplemented { version } => {
            format!("safescope {version} — コマンドラインはまだ実装されていません（M1 予定）。")
        }
    }
}
