//! Russian messages.

use crate::dataformatting::Msg;

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
