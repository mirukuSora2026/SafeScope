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
