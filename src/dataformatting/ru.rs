//! Russian messages.

use crate::dataformatting::Msg;

mod labels;

pub(super) fn render(msg: &Msg) -> String {
    match msg {
        Msg::PathEmpty => "Путь пуст.".to_owned(),
        Msg::PathTooLong { len, limit } => {
            format!("Путь слишком длинный ({len} байт, предел — {limit}).")
        }
        Msg::PathAbsolute => "Абсолютные пути не принимаются. Используйте путь относительно корня \
             рабочего пространства."
            .to_owned(),
        Msg::PathEmptyComponent { path } => format!(
            "В пути есть пустой компонент; проверьте '/' в начале, в конце или подряд: {path:?}"
        ),
        Msg::PathRelativeComponent { component } => format!(
            "Компонент '{component}' не допускается. Принимаются только плоские пути \
             относительно корня рабочего пространства."
        ),
        Msg::PathComponentTooLong { len, limit } => {
            format!("Компонент пути слишком длинный ({len} байт, предел — {limit}).")
        }
        Msg::PathControlCharacter { codepoint } => {
            format!("Путь содержит управляющий символ (U+{codepoint:04X}).")
        }
        Msg::PathBackslash => {
            "Компонент пути не может содержать '\\'. Разделитель — '/'.".to_owned()
        }
        Msg::PathTrailingDotOrSpace { component } => {
            format!("Компонент пути не может заканчиваться на '.' или пробел: {component:?}")
        }
        Msg::PathReservedDeviceName { component } => {
            format!("'{component}' — зарезервированное имя устройства.")
        }
        Msg::PathReservedPrefix { prefix } => {
            format!("Имена, начинающиеся с '{prefix}', зарезервированы движком.")
        }
        Msg::PathReservedPrefixHint => "Выберите другое имя файла.".to_owned(),
        Msg::PathTooDeep { depth, limit } => {
            format!("Путь слишком глубокий ({depth} уровней, предел — {limit}).")
        }
        Msg::PatternEmpty => "Шаблон политики пуст.".to_owned(),
        Msg::PatternAbsolute { pattern } => format!(
            "Шаблон политики {pattern:?} абсолютный. Шаблоны задаются относительно \
             корня рабочего пространства."
        ),
        Msg::PatternTraversal { pattern } => {
            format!("Шаблон политики {pattern:?} содержит '..', что не допускается.")
        }
        Msg::PatternInvalidGlob { pattern, reason } => {
            format!("Шаблон политики {pattern:?} не является корректным glob: {reason}")
        }
        Msg::PolicyParseFailed { reason } => {
            format!("Не удалось прочитать файл политики: {reason}")
        }
        Msg::PolicySchemaUnsupported { found, supported } => format!(
            "Неподдерживаемая версия схемы политики {found}; эта сборка понимает {supported}."
        ),
        Msg::Label(label) => labels::render(*label).to_owned(),
        Msg::WorkspaceAlreadyRegistered { root } => {
            format!("{root} уже является рабочим пространством SafeScope.")
        }
        Msg::WorkspaceNotRegistered { root } => {
            format!("{root} не является рабочим пространством SafeScope.")
        }
        Msg::WorkspaceIdCorrupted { path } => {
            format!("Идентификатор рабочего пространства в {path} нечитаем.")
        }
        Msg::WorkspaceRegistered { root, id } => {
            format!("{root} зарегистрирован как рабочее пространство {id}.")
        }
        Msg::WorkspaceBusyElsewhere { path } => format!(
            "В это рабочее пространство пишет другой процесс SafeScope (его блокировка — \
             {path}). Писатель одновременно только один, чтобы между проверкой бюджета и \
             следующим за ней резервированием не вклинился кто-то ещё."
        ),
        Msg::HintAnotherSessionIsWriting => {
            "Дождитесь завершения другого сеанса или закройте его.".to_owned()
        }
        Msg::HintRunInitFirst => "Сначала выполните `safescope init` в проекте.".to_owned(),
        Msg::HintFillInAllowThenApprove { policy } => format!(
            "Пока ничего нельзя изменить. Добавьте разрешаемые пути в {policy}, затем \
             выполните `safescope policy approve`."
        ),
        Msg::ExpansionPrompt {
            paths,
            operations,
            reason,
        } => format!(
            "SafeScope просит расширить то, что ему разрешено менять.\n\n\
             Пути: {paths}\n\
             Операции: {operations}\n\
             Указанная причина: {reason}\n\n\
             Одобрение откроет ровно эти пути, только для этой задачи и на ограниченное \
             время. Политику оно не меняет."
        ),
        Msg::ExpansionAlreadyAllowed { path } => {
            format!("{path} уже входит в разрешённую область; одобрять нечего.")
        }
        Msg::ExpansionCannotBeGranted { path } => format!(
            "{path} нельзя открыть одобрением. Защищённый путь или правило deny — не вопрос \
             разрешения, поэтому вопрос лишь отнял бы у кого-то внимание."
        ),
        Msg::ExpansionNeedsTerminal => "Этот клиент не может задать вопрос человеку, поэтому \
получить одобрение через него нельзя."
            .to_owned(),
        Msg::ExpansionLimitReached { used, limit } => format!(
            "Эта задача уже получила {used} из {limit} одобрений через клиент. Дальнейшие \
             нужно давать в терминале, чтобы длинная череда запросов не стала тихо широкой."
        ),
        Msg::ExpansionDeclined => "Запрос отклонён. Ничего не открыто.".to_owned(),
        Msg::ExpansionGranted { paths, minutes } => {
            format!("Одобрено: путей — {paths}, только для этой задачи, на {minutes} мин.")
        }
        Msg::HintApproveAtATerminal { paths } => {
            format!(
                "Выполните `safescope approve {paths}` в терминале, чтобы одобрить самостоятельно."
            )
        }
        Msg::McpInstructions => "SafeScope удерживает изменения файлов в пределах \
утверждённой области и лимитов и записывает их, чтобы их можно было просмотреть и отменить.\n\n\
Вызовите prepare_change, чтобы проверить и подготовить изменение — он ничего не меняет. \
Затем вызовите apply_change с полученным идентификатором плана. В отказе названо \
ответственное правило и сказано, может ли расширение области изменить ответ; если сказано, \
что не может, не повторяйте тот же запрос.\n\n\
SafeScope знает только об изменениях, сделанных через эти инструменты. Записанное командой \
оболочки не фиксируется и не может быть отменено здесь."
            .to_owned(),
        Msg::McpCoverageNotice => "Записываются только изменения, сделанные через SafeScope. \
Файлы, записанные командой оболочки или другим инструментом, не охвачены и не могут быть \
отменены здесь."
            .to_owned(),
        Msg::McpMissingField { field } => {
            format!("Для этой операции нужно поле {field}, но оно не передано.")
        }
        Msg::McpUnknownPlan { plan } => format!(
            "Подготовленного плана {plan} нет. Возможно, он уже применён или сервер с тех \
             пор перезапускался."
        ),
        Msg::McpTransportFailed { reason } => {
            format!("Не удалось обслужить MCP-соединение: {reason}")
        }
        Msg::HookUnsettledWork { count } => format!(
            "У SafeScope есть {count} операц. с незавершённой записью. Выполните \
             `safescope recover`, прежде чем полагаться на то, что лежит на диске."
        ),
        Msg::HookNeedsAttention { count } => format!(
            "SafeScope не может сказать, чем закончились {count} операц.; их должен сверить \
             человек. Автоматически ничего не исправлено."
        ),
        Msg::HookPolicyEdited => "Файл политики SafeScope изменён, но не утверждён, поэтому \
изменение не действует. Если оно задумано, выполните `safescope policy approve`."
            .to_owned(),
        Msg::HookChangedOutside { count } => format!(
            "Файлов изменено в обход SafeScope: {count}. Прежнее содержимое не сохранено, \
             отменить изменения нельзя. Выполните `safescope status`, чтобы увидеть какие."
        ),
        Msg::HookToolNotAllowed { tool } => format!(
            "Утверждённая политика запускает SafeScope в режиме списка разрешений, а `{tool}` \
             в списке нет. Меняйте файлы через SafeScope, чтобы изменение было записано."
        ),
        Msg::DriftNoBaseline => {
            "Базовое состояние не записывалось, поэтому SafeScope не может сказать, \
что изменилось помимо него."
                .to_owned()
        }
        Msg::DriftClean { scanned } => {
            format!("Ничего не изменено в обход SafeScope (проверено файлов: {scanned}).")
        }
        Msg::DriftUnrecoverable { count } => format!(
            "Из них {count} не имеют сохранённого прежнего содержимого. SafeScope не может их отменить."
        ),
        Msg::DriftTruncated { scanned } => {
            format!("Проверены только первые {scanned} файлов, поэтому список может быть неполным.")
        }
        Msg::HintReviewDrift => {
            "Просмотрите их, затем выполните `safescope drift accept`, чтобы принять \
текущее состояние как новое базовое."
                .to_owned()
        }
        Msg::HintAllowlistMode { allowed } => {
            format!("Инструменты, разрешённые политикой: {allowed}")
        }
        Msg::GuardUnsupportedHere => "Защищённый запуск требует песочницы ядра, а на этой платформе её нет.".to_owned(),
        Msg::HintGuardNeedsSandbox => "`safescope guard` работает на macOS через sandbox-exec. В других системах \
запустите команду без него и посмотрите `safescope drift`, чтобы увидеть, что изменилось помимо движка."
            .to_owned(),
        Msg::GuardStarting { path } => format!(
            "Защищается {path}. Ничто из запускаемого этой командой не сможет туда писать; \
             изменения должны идти через SafeScope."
        ),
        Msg::DurabilityLimitedHere => "Эта платформа не умеет сбрасывать каталог на диск, поэтому сбой сразу после \
изменения может его потерять. Файл наполовину записанным не останется, а найденное \
покажет `safescope recover`."
            .to_owned(),
        Msg::PolicyAllowToolsWithoutAllowlist => "`allow_tools` задан, но режим — `audit`, где ничего не отклоняется по имени, \
поэтому список ни на что не влияет. Если так и задумано, укажите `mode = \"allowlist\"`."
            .to_owned(),
        Msg::PolicyAllowToolsReopensTheGap { tool } => format!(
            "В `allow_tools` указан `{tool}`, умеющий выполнять команды оболочки. Разрешив \
             его, вы снова открываете путь, который этот режим и должен закрывать; такие \
             изменения не записываются."
        ),
        Msg::HintNothingToUndo => "В этой задаче нечего отменять. Отмена идёт по одной операции за раз и только по \
изменениям, сделанным через SafeScope; для изменения в обход движка возвращать нечего."
            .to_owned(),
        Msg::HintWhatTheOperationNeeds { operation, field } => format!(
            "Для `{operation}` нужно `{field}`. Отправьте его ровно с этим именем — \
             похожее имя считается отсутствующим полем, а не другим написанием того же."
        ),
        Msg::HintSnapshotMayHaveAged => "Данные для восстановления хранятся `retain_closed_task_days` после завершения \
задачи, и это может быть старше. История того, что произошло, сохранена полностью; нет только \
содержимого, нужного чтобы вернуть всё назад."
            .to_owned(),
        Msg::JournalPragmaRefused { pragma, wanted, found } => format!(
            "Журнал не удалось перевести в `{pragma} = {wanted}`; сейчас это `{found}`. \
             На этой настройке держатся все обещания пережить сбой, поэтому журнал не откроется, \
             а не станет обещать то, чего не может."
        ),
        Msg::HintJournalNeedsARealFilesystem => "Обычно это значит, что каталог состояния лежит на файловой системе, которая так не умеет — \
сетевой ресурс или некоторые монтирования контейнеров. Укажите SAFESCOPE_DATA_DIR на локальное хранилище."
            .to_owned(),
        Msg::RecoveryFoundNothing => "Восстанавливать было нечего.".to_owned(),
        Msg::RecoverySettled { aborted, committed } => {
            format!("Разрешено операций: выполненных — {committed}, невыполненных — {aborted}.")
        }
        Msg::RecoveryLeftUnresolved { count } => format!(
            "Операций, которые не удалось разрешить: {count}. То, что на диске, не совпадает \
             ни с прежним состоянием, ни с тем, что дало бы изменение. Ничего не исправлено — \
             догадки здесь и уничтожают целую работу."
        ),
        Msg::RecoveryRemovedTemporaries { count } => {
            format!("Удалено временных файлов, оставленных сбоем: {count}.")
        }
        Msg::DoctorHealthy => "Всё проверенное здесь в порядке.".to_owned(),
        Msg::UndoNothingRecorded => "Отменять нечего: у этой задачи нет завершённых \
операций. Операцию с неясным исходом нужно сначала разрешить."
            .to_owned(),
        Msg::UndoConflictAt { path } => format!(
            "{path} изменился после того, как SafeScope трогал его в последний раз, поэтому \
             отмена затёрла бы появившееся позже. Ничего не сделано."
        ),
        Msg::HintCompareBeforeUndoing { path } => format!(
            "Сравните {path} с сохранённым прежним содержимым и решите, что оставить; \
             данные восстановления сохраняются в любом случае."
        ),
        Msg::BudgetPathsExceeded {
            used,
            limit,
            adding,
        } => format!(
            "Эта задача уже изменила {used} из {limit} разрешённых путей, а {adding} стал бы \
             ещё одним. Ничего не сделано."
        ),
        Msg::BudgetOperationsExceeded { used, limit } => {
            format!("Эта задача использовала {used} из {limit} разрешённых операций.")
        }
        Msg::BudgetMovesExceeded { used, limit } => {
            format!("Эта задача использовала {used} из {limit} разрешённых перемещений.")
        }
        Msg::BudgetStorageExceeded { used, limit } => format!(
            "Данные восстановления этого рабочего пространства занимают {used} при пределе \
             {limit}, поэтому содержимое, которое уничтожит это изменение, негде сохранить."
        ),
        Msg::HintRequestBudgetExpansion => "Просмотрите уже сделанные изменения. Если работе \
действительно нужно больше, попросите поднять предел, а не повторяйте попытку."
            .to_owned(),
        Msg::PlanTargetExists { path } => format!(
            "{path} уже существует, поэтому его нельзя создать. Замена — другая операция, \
             и она сначала сохраняет прежнее содержимое."
        ),
        Msg::PlanTargetMissing { path } => {
            format!("{path} не существует, поэтому операции не над чем работать.")
        }
        Msg::PlanFileTooLarge { path, size, limit } => {
            format!("{path} занимает {size}, что превышает разрешённые политикой {limit}.")
        }
        Msg::PlanHasExpired { plan } => {
            format!("Срок действия плана {plan} истёк, он не будет применён.")
        }
        Msg::PlanStateChanged { path } => format!(
            "{path} изменился после составления плана. Ничего не сделано: применение плана, \
             построенного по другому содержимому, затёрло бы то, что появилось в промежутке."
        ),
        Msg::HintRebuildThePlan => "Составьте план заново по текущему содержимому.".to_owned(),
        Msg::JournalOpenFailed { path, reason } => {
            format!("Не удалось открыть журнал в {path}: {reason}")
        }
        Msg::JournalOperationFailed { reason } => {
            format!("Не удалось записать журнал: {reason}")
        }
        Msg::JournalRequestMismatch { request } => format!(
            "Запрос {request} уже записан с другим содержимым. Он сообщается, а не считается \
             новым: догадка о том, что имелось в виду, может применить изменение дважды."
        ),
        Msg::JournalUnknownOperation { operation } => {
            format!("Для операции {operation} нет записи в журнале.")
        }
        Msg::SnapshotVerificationFailed { hash } => format!(
            "Данные восстановления для {hash} не сходятся по хешу с тем, что было сохранено. \
             Ничего не изменено: без снимка, который читается обратно верно, изменение \
             нельзя было бы отменить."
        ),
        Msg::SnapshotMissing { hash } => {
            format!("Данных восстановления для {hash} нет в хранилище.")
        }
        Msg::SnapshotStoreFailed { reason } => {
            format!("Не удалось сохранить данные восстановления: {reason}. Оригинал не тронут.")
        }
        Msg::StoreDataDirectoryUnavailable => "Не найден каталог для состояния движка. \
Укажите его через SAFESCOPE_DATA_DIR."
            .to_owned(),
        Msg::StoreWriteFailed { path, reason } => {
            format!("Не удалось записать состояние движка в {path}: {reason}")
        }
        Msg::StoreReadFailed { path, reason } => {
            format!("Не удалось прочитать состояние движка из {path}: {reason}")
        }
        Msg::StoreCorrupted { path, reason } => format!(
            "Состояние движка в {path} нечитаемо: {reason}. Оно не было исправлено \
             автоматически: догадки о повреждённом состоянии — это путь к потере целых данных."
        ),
        Msg::StoreStateInsideWorkspace { state, workspace } => format!(
            "Состояние движка оказалось бы в {state}, внутри рабочего пространства \
             {workspace}. Тогда данные восстановления лежали бы внутри дерева, которое они \
             защищают, поэтому регистрация отклонена."
        ),
        Msg::PathDestinationExists { path } => format!(
            "{path} уже существует. Перемещение никогда не перезаписывает цель: у файла, \
             который был бы потерян, нет данных восстановления."
        ),
        Msg::PlatformCrossFilesystem { from, to } => format!(
            "{from} и {to} находятся в разных файловых системах. Перемещение между ними не \
             может быть атомарным, поэтому оно отклоняется, а не выполняется копированием \
             с удалением."
        ),
        Msg::PlatformAtomicRenameUnsupported { reason } => format!(
            "Эта файловая система не умеет переименовывать без перезаписи ({reason}). \
             Проверка перед переименованием вернула бы ту самую гонку, которую операция \
             и призвана закрыть, поэтому операция отклоняется."
        ),
        Msg::PlatformOperationFailed { operation, reason } => {
            format!("Операция {operation} не удалась: {reason}")
        }
        Msg::WorkspaceOpenFailed { root, reason } => {
            format!("Не удалось открыть рабочее пространство в {root}: {reason}")
        }
        Msg::PathNotARegularFile { path } => format!(
            "{path} не является обычным файлом. Эта версия работает только с обычными файлами."
        ),
        Msg::PathComponentNotADirectory { path, component } => {
            format!("При разборе {path} компонент {component:?} не является каталогом.")
        }
        Msg::PathSymlinkRefused { path } => format!(
            "{path} — символическая ссылка. Переход по ней вывел бы операцию за пределы \
             проверенного пути, поэтому она отклоняется, а не разрешается."
        ),
        Msg::PathParentMissing { path, parent } => {
            format!("{path} нельзя создать: каталог {parent} не существует.")
        }
        Msg::HintCreateTheDirectoryFirst => "Эта версия не создаёт каталоги. Создайте его \
самостоятельно и повторите."
            .to_owned(),
        Msg::ScopeNotCovered { path } => {
            format!("{path} не подпадает ни под одно правило разрешённой области.")
        }
        Msg::ScopeDeniedByRule { path, pattern } => {
            format!("{path} отклонён правилом deny {pattern:?}.")
        }
        Msg::ScopeOperationNotAllowed {
            path,
            operation,
            allowed,
        } => format!(
            "{path} можно изменять, но не операцией {operation}. Здесь разрешено: {allowed}."
        ),
        Msg::HintExpansionMayBeRequested => "Если этот путь действительно нужен, запросите \
расширение области через safescope, а не повторяйте ту же правку."
            .to_owned(),
        Msg::HintPolicyDenyIsFinal => "Правило deny нельзя снять одобрением. Если это ошибка, \
измените файл политики и утвердите его."
            .to_owned(),
        Msg::PolicyNoAllowRules => "В политике нет правил allow, поэтому ничего нельзя \
изменить. Если это намеренно, укажите это явно, а не оставляйте список пустым."
            .to_owned(),
        Msg::PolicyEmptyDefaultOps => "default_ops пуст, поэтому краткие правила allow не \
дадут ни одной операции."
            .to_owned(),
        Msg::PolicyRuleGrantsNothing { pattern } => {
            format!("Правило {pattern:?} не разрешает ни одной операции.")
        }
        Msg::PolicyAllowOverProtected { pattern, protected } => format!(
            "Правило allow {pattern:?} затрагивает {protected:?}, который движок защищает \
             всегда, поэтому правило никогда не сработает."
        ),
        Msg::PolicyWorkspaceWideNeedsOptIn { pattern } => format!(
            "Правило allow {pattern:?} охватывает всё рабочее пространство. Если это \
             намеренно, установите safety.unsafe_allow_workspace_wide = true."
        ),
        Msg::PolicyAllowAlsoDenied { pattern } => format!(
            "{pattern:?} присутствует и в allow, и в deny. Deny всегда побеждает, поэтому \
             намерение неясно."
        ),
        Msg::PolicyDuplicatePattern { pattern } => {
            format!("Шаблон {pattern:?} указан несколько раз.")
        }
        Msg::PolicyDenyNeverApplies { pattern } => format!(
            "Правило deny {pattern:?} не пересекается ни с одним allow, поэтому не действует."
        ),
        Msg::PolicyBudgetZero { field } => {
            format!("budget.{field} равен нулю, что отклонит любую операцию.")
        }
        Msg::PolicyFileLimitExceedsSnapshotLimit {
            file_bytes,
            snapshot_bytes,
        } => format!(
            "max_file_bytes ({file_bytes}) превышает max_snapshot_bytes ({snapshot_bytes}), \
             поэтому для разрешённого файла нельзя сохранить данные восстановления."
        ),
        Msg::PolicyWarnRatioOutOfRange { value } => {
            format!("budget.warn_at_ratio равен {value}; он должен быть между 0 и 1.")
        }
        Msg::ProtectedEngineState => "Собственная политика движка и состояние рабочего \
пространства. Возможность их изменить позволила бы движку переписать то, по чему он проверяет."
            .to_owned(),
        Msg::ProtectedGitHistory => "История Git. Логика восстановления исходит из её \
целостности; если её можно переписать, решения о конфликтах теряют смысл."
            .to_owned(),
        Msg::ProtectedPermissionSurface => "Настройки разрешений Claude Code. Доступ на \
запись сюда позволил бы снять ограничения инструментов изнутри."
            .to_owned(),
        Msg::ProtectedTemporaryName => {
            "Имя, зарезервированное для атомарной замены движка.".to_owned()
        }
        Msg::HashBadLength { len } => {
            format!("Неверная длина хеша ({len} символов, требуется 64).")
        }
        Msg::HashNotHexadecimal { text } => {
            format!("Хеш содержит не шестнадцатеричное значение: {text:?}")
        }
        Msg::FaultUnknownValue { variable, value } => {
            format!("Нераспознанное значение {variable}: {value:?}")
        }
        Msg::FaultAborting { point } => {
            format!("Прерывание во внедрённой точке сбоя {point}.")
        }
        Msg::CliNotImplemented { version } => {
            format!("safescope {version} — командная строка ещё не реализована (планируется в M1).")
        }
    }
}
