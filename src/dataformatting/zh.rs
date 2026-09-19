//! Simplified Chinese messages.

use crate::dataformatting::Msg;

pub(super) fn render(msg: &Msg) -> String {
    match msg {
        Msg::PathEmpty => "路径为空。".to_owned(),
        Msg::PathTooLong { len, limit } => {
            format!("路径过长（{len} 字节，上限为 {limit}）。")
        }
        Msg::PathAbsolute => "不接受绝对路径。请使用相对于工作区根目录的路径。".to_owned(),
        Msg::PathEmptyComponent { path } => {
            format!("路径中存在空的组成部分，请检查开头、结尾或重复的 '/'：{path:?}")
        }
        Msg::PathRelativeComponent { component } => {
            format!("不允许使用 '{component}' 组成部分。只接受相对于工作区根目录的扁平路径。")
        }
        Msg::PathComponentTooLong { len, limit } => {
            format!("路径组成部分过长（{len} 字节，上限为 {limit}）。")
        }
        Msg::PathControlCharacter { codepoint } => {
            format!("路径中包含控制字符（U+{codepoint:04X}）。")
        }
        Msg::PathBackslash => "路径组成部分中不能使用 '\\'。分隔符是 '/'。".to_owned(),
        Msg::PathTrailingDotOrSpace { component } => {
            format!("路径组成部分不能以 '.' 或空格结尾：{component:?}")
        }
        Msg::PathReservedDeviceName { component } => {
            format!("'{component}' 是保留的设备名称。")
        }
        Msg::PathReservedPrefix { prefix } => {
            format!("以 '{prefix}' 开头的名称由引擎保留。")
        }
        Msg::PathReservedPrefixHint => "请换一个文件名。".to_owned(),
        Msg::PathTooDeep { depth, limit } => {
            format!("路径层级过深（{depth} 层，上限为 {limit}）。")
        }
        Msg::PatternEmpty => "存在空的策略模式。".to_owned(),
        Msg::PatternAbsolute { pattern } => {
            format!("策略模式 {pattern:?} 是绝对路径。模式相对于工作区根目录。")
        }
        Msg::PatternTraversal { pattern } => {
            format!("策略模式 {pattern:?} 包含 '..'，这是不允许的。")
        }
        Msg::PatternInvalidGlob { pattern, reason } => {
            format!("策略模式 {pattern:?} 不是有效的 glob：{reason}")
        }
        Msg::PolicyParseFailed { reason } => {
            format!("无法读取策略文件：{reason}")
        }
        Msg::PolicySchemaUnsupported { found, supported } => {
            format!("不支持的策略 schema_version {found}；此版本支持 {supported}。")
        }
        Msg::ScopeNotCovered { path } => {
            format!("{path} 不匹配允许范围中的任何规则。")
        }
        Msg::ScopeDeniedByRule { path, pattern } => {
            format!("{path} 被 deny 规则 {pattern:?} 拒绝。")
        }
        Msg::ScopeOperationNotAllowed {
            path,
            operation,
            allowed,
        } => {
            format!("{path} 可以修改，但不允许 {operation}。此处允许的操作：{allowed}。")
        }
        Msg::HintExpansionMayBeRequested => "如果确实需要此路径，请通过 safescope 申请扩大范围，\
而不是重试同一次编辑。"
            .to_owned(),
        Msg::HintPolicyDenyIsFinal => "deny 规则无法通过审批解除。如果这是错误的，\
请修改策略文件并重新批准。"
            .to_owned(),
        Msg::PolicyNoAllowRules => "策略中没有 allow 规则，因此什么都无法更改。\
如果这是有意的，请明确说明，而不是留空列表。"
            .to_owned(),
        Msg::PolicyEmptyDefaultOps => {
            "default_ops 为空，简写形式的 allow 规则将不授予任何操作。".to_owned()
        }
        Msg::PolicyRuleGrantsNothing { pattern } => {
            format!("规则 {pattern:?} 不授予任何操作。")
        }
        Msg::PolicyAllowOverProtected { pattern, protected } => {
            format!("allow 规则 {pattern:?} 触及引擎始终保护的 {protected:?}，该规则永远不会生效。")
        }
        Msg::PolicyWorkspaceWideNeedsOptIn { pattern } => format!(
            "allow 规则 {pattern:?} 覆盖整个工作区。如果确属有意，请设置 \
             safety.unsafe_allow_workspace_wide = true。"
        ),
        Msg::PolicyAllowAlsoDenied { pattern } => {
            format!("{pattern:?} 同时出现在 allow 和 deny 中。deny 始终优先，意图不明确。")
        }
        Msg::PolicyDuplicatePattern { pattern } => {
            format!("模式 {pattern:?} 出现了多次。")
        }
        Msg::PolicyDenyNeverApplies { pattern } => {
            format!("deny 规则 {pattern:?} 与任何 allow 规则都不重叠，因此没有效果。")
        }
        Msg::PolicyBudgetZero { field } => {
            format!("budget.{field} 为 0，将拒绝所有操作。")
        }
        Msg::PolicyFileLimitExceedsSnapshotLimit {
            file_bytes,
            snapshot_bytes,
        } => format!(
            "max_file_bytes（{file_bytes}）超过 max_snapshot_bytes（{snapshot_bytes}），\
             允许的文件将无法保存恢复数据。"
        ),
        Msg::PolicyWarnRatioOutOfRange { value } => {
            format!("budget.warn_at_ratio 为 {value}，必须介于 0 和 1 之间。")
        }
        Msg::ProtectedEngineState => "引擎自身的策略与工作区状态。\
若可修改，引擎便能改写自己的判断依据。"
            .to_owned(),
        Msg::ProtectedGitHistory => "Git 历史。恢复判定以历史完整为前提，\
若可被改写，冲突判定将失去意义。"
            .to_owned(),
        Msg::ProtectedPermissionSurface => "Claude Code 的权限设置。\
若可写入，就能从内部解除工具限制。"
            .to_owned(),
        Msg::ProtectedTemporaryName => "引擎原子替换所保留的名称。".to_owned(),
        Msg::HashBadLength { len } => {
            format!("哈希长度不正确（{len} 个字符，应为 64 个）。")
        }
        Msg::HashNotHexadecimal { text } => {
            format!("哈希中包含非十六进制的值：{text:?}")
        }
        Msg::FaultUnknownValue { variable, value } => {
            format!("无法识别的 {variable} 值：{value:?}")
        }
        Msg::FaultAborting { point } => {
            format!("在注入的故障点 {point} 处中止。")
        }
        Msg::CliNotImplemented { version } => {
            format!("safescope {version} — 命令行尚未实现（计划在 M1 阶段）。")
        }
    }
}
