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
