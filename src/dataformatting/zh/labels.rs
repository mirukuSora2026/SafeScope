//! The zh spellings of the fixed labels.
//!
//! Split from the messages beside them only for length. It is still one
//! exhaustive match on `Label`, so a new label still fails to compile until
//! every language has one.

use crate::dataformatting::Label;

pub(super) fn render(label: Label) -> &'static str {
    match label {
        Label::Allowed => "允许",
        Label::Refused => "拒绝",
        Label::NotCovered => "未覆盖",
        Label::Path => "路径",
        Label::Operation => "操作",
        Label::Policy => "策略",
        Label::PolicyVersion => "版本",
        Label::EvaluationSteps => "评估过程",
        Label::StepProtected => "受保护路径",
        Label::StepDeny => "deny 规则",
        Label::StepAllow => "allow 规则",
        Label::StepGrant => "临时批准",
        Label::NoMatch => "无匹配",
        Label::Outcome => "结果",
        Label::ExpansionPossible => "可以申请扩大范围",
        Label::ExpansionImpossible => "无法通过批准解除",
        Label::CurrentScope => "允许范围",
        Label::Nothing => "无",
        Label::Warnings => "警告",
        Label::Approved => "已批准",
        Label::UnapprovedEdits => "策略文件有未批准的修改",
        Label::Task => "任务",
        Label::Usage => "用量",
        Label::State => "状态",
        Label::LastChange => "最近更改",
        Label::ChangedPaths => "变更路径",
        Label::Operations => "文件操作",
        Label::Moves => "移动",
        Label::RecoveryStorage => "恢复存储",
        Label::Unfinished => "未完成",
        Label::NeedsComparing => "需要比对",
        Label::TemporaryApprovals => "临时批准",
        Label::PolicyFile => "策略文件",
        Label::History => "历史",
        Label::Coverage => "覆盖范围",
        Label::NotStarted => "尚未开始任务",
        Label::NoPolicyYet => "尚未批准策略",
        Label::Checks => "检查",
        Label::Passed => "正常",
        Label::Failed => "失败",
        Label::ChangedOutside => "在 SafeScope 之外被更改",
        Label::NoBaseline => "无基准",
        Label::DriftAdded => "新增",
        Label::DriftModified => "更改",
        Label::DriftRemoved => "删除",
    }
}
