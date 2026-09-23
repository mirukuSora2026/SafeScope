//! The ja spellings of the fixed labels.
//!
//! Split from the messages beside them only for length. It is still one
//! exhaustive match on `Label`, so a new label still fails to compile until
//! every language has one.

use crate::dataformatting::Label;

pub(super) fn render(label: Label) -> &'static str {
    match label {
        Label::Allowed => "許可",
        Label::Refused => "拒否",
        Label::NotCovered => "該当なし",
        Label::Path => "パス",
        Label::Operation => "操作",
        Label::Policy => "ポリシー",
        Label::PolicyVersion => "バージョン",
        Label::EvaluationSteps => "評価",
        Label::StepProtected => "保護パス",
        Label::StepDeny => "deny ルール",
        Label::StepAllow => "allow ルール",
        Label::StepGrant => "一時承認",
        Label::NoMatch => "一致なし",
        Label::Outcome => "結果",
        Label::ExpansionPossible => "範囲の拡張を要求できます",
        Label::ExpansionImpossible => "承認では解除できません",
        Label::CurrentScope => "許可範囲",
        Label::Nothing => "なし",
        Label::Warnings => "警告",
        Label::Approved => "承認済み",
        Label::UnapprovedEdits => "ポリシーファイルに未承認の変更があります",
        Label::Task => "タスク",
        Label::Usage => "使用量",
        Label::State => "状態",
        Label::LastChange => "最後の変更",
        Label::ChangedPaths => "変更パス",
        Label::Operations => "ファイル操作",
        Label::Moves => "移動",
        Label::RecoveryStorage => "復旧ストレージ",
        Label::Unfinished => "未完了",
        Label::NeedsComparing => "照合が必要",
        Label::TemporaryApprovals => "一時承認",
        Label::PolicyFile => "ポリシーファイル",
        Label::History => "履歴",
        Label::Coverage => "適用範囲",
        Label::NotStarted => "タスク未開始",
        Label::NoPolicyYet => "ポリシー未承認",
        Label::Checks => "点検",
        Label::Passed => "正常",
        Label::Failed => "失敗",
        Label::ChangedOutside => "SafeScope の外で変更されたもの",
        Label::NoBaseline => "基準なし",
        Label::DriftAdded => "追加",
        Label::DriftModified => "変更",
        Label::DriftRemoved => "削除",
    }
}
