//! Simplified Chinese messages.

use crate::dataformatting::{Label, Msg};

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
        Msg::Label(label) => match label {
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
        }
        .to_owned(),
        Msg::WorkspaceAlreadyRegistered { root } => {
            format!("{root} 已经是 SafeScope 工作区。")
        }
        Msg::WorkspaceNotRegistered { root } => {
            format!("{root} 不是 SafeScope 工作区。")
        }
        Msg::WorkspaceIdCorrupted { path } => {
            format!("无法读取位于 {path} 的工作区标识。")
        }
        Msg::WorkspaceRegistered { root, id } => {
            format!("已将 {root} 注册为工作区 {id}。")
        }
        Msg::HintRunInitFirst => "请先在项目中运行 `safescope init`。".to_owned(),
        Msg::HintFillInAllowThenApprove { policy } => {
            format!(
                "目前还不能更改任何内容。请在 {policy} 中填写要允许的路径，然后运行 `safescope policy approve`。"
            )
        }
        Msg::JournalOpenFailed { path, reason } => {
            format!("无法打开位于 {path} 的作业记录：{reason}")
        }
        Msg::JournalOperationFailed { reason } => {
            format!("无法写入作业记录：{reason}")
        }
        Msg::JournalRequestMismatch { request } => format!(
            "请求 {request} 已以不同内容记录在案。猜测其本意可能导致同一更改被应用两次，\
             因此予以报告，而不是当作新请求。"
        ),
        Msg::JournalUnknownOperation { operation } => {
            format!("没有关于操作 {operation} 的作业记录。")
        }
        Msg::SnapshotVerificationFailed { hash } => format!(
            "{hash} 的恢复数据重新计算的哈希与所存的不一致。未做任何更改：\
             没有能正确读回的快照，这次更改将无法撤销。"
        ),
        Msg::SnapshotMissing { hash } => {
            format!("存储中没有 {hash} 的恢复数据。")
        }
        Msg::SnapshotStoreFailed { reason } => {
            format!("无法保存恢复数据：{reason}。原文件保持不变。")
        }
        Msg::StoreDataDirectoryUnavailable => "找不到用于存放引擎状态的目录。\
请设置 SAFESCOPE_DATA_DIR 指定一个。"
            .to_owned(),
        Msg::StoreWriteFailed { path, reason } => {
            format!("无法将引擎状态写入 {path}：{reason}")
        }
        Msg::StoreReadFailed { path, reason } => {
            format!("无法从 {path} 读取引擎状态：{reason}")
        }
        Msg::StoreCorrupted { path, reason } => format!(
            "位于 {path} 的引擎状态无法读取：{reason}。未自动修复，\
             因为对损坏状态的猜测正是丢失完好数据的途径。"
        ),
        Msg::StoreStateInsideWorkspace { state, workspace } => format!(
            "引擎状态将位于 {state}，处于工作区 {workspace} 之内。\
             那样恢复数据就会落在它所保护的目录树中，因此拒绝注册。"
        ),
        Msg::PathDestinationExists { path } => {
            format!("{path} 已存在。移动从不覆盖目标，因为将被丢失的文件没有恢复数据。")
        }
        Msg::PlatformCrossFilesystem { from, to } => format!(
            "{from} 与 {to} 位于不同的文件系统。两者之间的移动无法保证原子性，\
             因此予以拒绝，而不是以复制加删除代替。"
        ),
        Msg::PlatformAtomicRenameUnsupported { reason } => format!(
            "此文件系统不支持不覆盖的重命名（{reason}）。先检查后重命名会重新引入\
             该操作本要消除的竞态，因此改为拒绝。"
        ),
        Msg::PlatformOperationFailed { operation, reason } => {
            format!("{operation} 操作失败：{reason}")
        }
        Msg::WorkspaceOpenFailed { root, reason } => {
            format!("无法打开位于 {root} 的工作区：{reason}")
        }
        Msg::PathNotARegularFile { path } => {
            format!("{path} 不是普通文件。此版本仅处理单个普通文件。")
        }
        Msg::PathComponentNotADirectory { path, component } => {
            format!("解析 {path} 时，{component:?} 不是目录。")
        }
        Msg::PathSymlinkRefused { path } => {
            format!("{path} 是符号链接。跟随它会让操作离开已检查的路径，因此予以拒绝而非解析。")
        }
        Msg::PathParentMissing { path, parent } => {
            format!("目录 {parent} 不存在，因此无法创建 {path}。")
        }
        Msg::HintCreateTheDirectoryFirst => "此版本不会创建目录。请自行创建后重试。".to_owned(),
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
