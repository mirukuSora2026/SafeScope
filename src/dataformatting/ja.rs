//! Japanese messages.

use crate::dataformatting::Msg;

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
