//! Korean messages.

use crate::dataformatting::Msg;

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
