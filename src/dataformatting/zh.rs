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
        Msg::WorkspaceBusyElsewhere { path } => format!(
            "另一个 SafeScope 进程正在写入此工作区（其锁位于 {path}）。\
             同一时间只允许一个写入者，以免预算检查与随之而来的预留被他人插入其间。"
        ),
        Msg::HintAnotherSessionIsWriting => "请等待另一个会话结束，或将其关闭。".to_owned(),
        Msg::HintRunInitFirst => "请先在项目中运行 `safescope init`。".to_owned(),
        Msg::HintFillInAllowThenApprove { policy } => {
            format!(
                "目前还不能更改任何内容。请在 {policy} 中填写要允许的路径，然后运行 `safescope policy approve`。"
            )
        }
        Msg::ExpansionPrompt {
            paths,
            operations,
            reason,
        } => format!(
            "SafeScope 正在请求扩大可更改的范围。\n\n\
             路径：{paths}\n\
             操作：{operations}\n\
             所给理由：{reason}\n\n\
             批准将仅为本任务、在有限时间内开放这些路径，并不会更改策略。"
        ),
        Msg::ExpansionAlreadyAllowed { path } => {
            format!("{path} 已在允许范围内，无需批准。")
        }
        Msg::ExpansionCannotBeGranted { path } => {
            format!(
                "{path} 无法通过批准开放。受保护路径或 deny 规则不是权限问题，询问只会浪费他人注意力。"
            )
        }
        Msg::ExpansionNeedsTerminal => {
            "此客户端无法向用户提问，因此无法通过它获得批准。".to_owned()
        }
        Msg::ExpansionLimitReached { used, limit } => format!(
            "此任务已通过客户端获得 {limit} 次中的 {used} 次批准。\
             之后的批准须在终端进行，以免一连串请求悄然变成一次大范围放开。"
        ),
        Msg::ExpansionDeclined => "请求已被拒绝，未开放任何内容。".to_owned(),
        Msg::ExpansionGranted { paths, minutes } => {
            format!("已批准：{paths} 个路径，仅限本任务，有效 {minutes} 分钟。")
        }
        Msg::HintApproveAtATerminal { paths } => {
            format!("可在终端运行 `safescope approve {paths}` 自行批准。")
        }
        Msg::McpInstructions => "SafeScope 将文件更改保持在已批准的范围与限额之内，\
并加以记录，以便复查和撤销。\n\n\
先调用 prepare_change 检查并准备更改，该步骤不改动任何内容；\
然后用返回的计划 ID 调用 apply_change。拒绝会说明是哪条规则所致，\
以及申请扩大范围是否可能改变结果；若说不能，请勿重试同一请求。\n\n\
SafeScope 只了解经由这些工具所做的更改。由 shell 命令写入的内容不会被记录，\
也无法在此撤销。"
            .to_owned(),
        Msg::McpCoverageNotice => "只有经由 SafeScope 的更改才会被记录。\
由 shell 命令或其他工具写入的文件不在覆盖范围内，无法在此撤销。"
            .to_owned(),
        Msg::McpMissingField { field } => {
            format!("此操作需要 {field}，但未提供。")
        }
        Msg::McpUnknownPlan { plan } => {
            format!("没有已准备的计划 {plan}。它可能已被执行，或服务器此后重启过。")
        }
        Msg::McpTransportFailed { reason } => {
            format!("无法提供 MCP 连接：{reason}")
        }
        Msg::HookUnsettledWork { count } => format!(
            "SafeScope 有 {count} 个操作尚未记录完毕。在依赖磁盘上的内容之前，\
             请运行 `safescope recover`。"
        ),
        Msg::HookNeedsAttention { count } => format!(
            "SafeScope 无法判定 {count} 个操作的结果，需要有人进行比对。未自动修复任何内容。"
        ),
        Msg::HookPolicyEdited => "SafeScope 策略文件已修改但未批准，因此更改尚未生效。\
如果确属有意，请运行 `safescope policy approve`。"
            .to_owned(),
        Msg::HookChangedOutside { count } => format!(
            "有 {count} 个文件未经 SafeScope 而被更改，因此没有快照，无法撤销。\
             运行 `safescope status` 查看是哪些文件。"
        ),
        Msg::HookToolNotAllowed { tool } => format!(
            "已批准的策略让 SafeScope 运行在允许列表模式，而 `{tool}` 不在列表中。\
             请通过 SafeScope 修改文件，以便更改被记录。"
        ),
        Msg::DriftNoBaseline => {
            "尚未记录基准状态，SafeScope 无法说明在它之外发生了什么更改。".to_owned()
        }
        Msg::DriftClean { scanned } => {
            format!("没有在 SafeScope 之外发生的更改（已检查 {scanned} 个文件）。")
        }
        Msg::DriftUnrecoverable { count } => {
            format!("其中 {count} 个没有保存先前内容，SafeScope 无法撤销。")
        }
        Msg::DriftTruncated { scanned } => {
            format!("仅检查了前 {scanned} 个文件，此列表可能并不完整。")
        }
        Msg::HintReviewDrift => {
            "检查之后，运行 `safescope drift accept` 将当前状态作为新的基准。".to_owned()
        }
        Msg::HintAllowlistMode { allowed } => format!("策略允许的工具：{allowed}"),
        Msg::RecoveryFoundNothing => "没有需要恢复的内容。".to_owned(),
        Msg::RecoverySettled { aborted, committed } => {
            format!("已确定 {committed} 个已执行的操作和 {aborted} 个未执行的操作。")
        }
        Msg::RecoveryLeftUnresolved { count } => format!(
            "有 {count} 个操作无法确定：磁盘上的内容既不同于此前状态，\
             也不同于该更改本应产生的结果。未做任何修复——在此处猜测正是毁掉完好工作的途径。"
        ),
        Msg::RecoveryRemovedTemporaries { count } => {
            format!("已删除崩溃遗留的 {count} 个临时文件。")
        }
        Msg::DoctorHealthy => "此处检查的项目均正常。".to_owned(),
        Msg::UndoNothingRecorded => "没有可撤销的内容：此任务没有已完成的操作。\
结果尚不明确的操作必须先行确定。"
            .to_owned(),
        Msg::UndoConflictAt { path } => {
            format!(
                "{path} 在 SafeScope 最后一次改动之后发生了变化，撤销会覆盖其后写入的内容。未做任何更改。"
            )
        }
        Msg::HintCompareBeforeUndoing { path } => {
            format!("请将 {path} 与所存的原有内容比较后决定保留哪一份；恢复数据都会保留。")
        }
        Msg::BudgetPathsExceeded {
            used,
            limit,
            adding,
        } => format!(
            "此任务已更改 {limit} 个允许路径中的 {used} 个，而 {adding} 将是又一个。\
             未做任何更改。"
        ),
        Msg::BudgetOperationsExceeded { used, limit } => {
            format!("此任务已使用 {limit} 次允许操作中的 {used} 次。")
        }
        Msg::BudgetMovesExceeded { used, limit } => {
            format!("此任务已使用 {limit} 次允许移动中的 {used} 次。")
        }
        Msg::BudgetStorageExceeded { used, limit } => {
            format!("此工作区的恢复数据为 {used}，上限为 {limit}，无法保存本次更改将销毁的内容。")
        }
        Msg::HintRequestBudgetExpansion => "请检查目前已更改的内容。\
如果确实需要更多，请申请提高上限，而不是重试。"
            .to_owned(),
        Msg::PlanTargetExists { path } => {
            format!("{path} 已存在，无法创建。替换是另一种操作，且会先保存原有内容。")
        }
        Msg::PlanTargetMissing { path } => {
            format!("{path} 不存在，此操作没有可作用的对象。")
        }
        Msg::PlanFileTooLarge { path, size, limit } => {
            format!("{path} 为 {size}，超过此策略允许的 {limit}。")
        }
        Msg::PlanHasExpired { plan } => {
            format!("计划 {plan} 已过期，不会执行。")
        }
        Msg::PlanStateChanged { path } => format!(
            "{path} 在制定计划之后发生了变化。未做任何更改：\
             应用基于不同内容制定的计划会覆盖这期间写入的内容。"
        ),
        Msg::HintRebuildThePlan => "请基于当前内容重新制定计划。".to_owned(),
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
