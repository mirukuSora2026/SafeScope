//! The ru spellings of the fixed labels.
//!
//! Split from the messages beside them only for length. It is still one
//! exhaustive match on `Label`, so a new label still fails to compile until
//! every language has one.

use crate::dataformatting::Label;

pub(super) fn render(label: Label) -> &'static str {
    match label {
        Label::Allowed => "Разрешено",
        Label::Refused => "Отклонено",
        Label::NotCovered => "Не покрыто",
        Label::Path => "Путь",
        Label::Operation => "Операция",
        Label::Policy => "Политика",
        Label::PolicyVersion => "Версия",
        Label::EvaluationSteps => "Разбор",
        Label::StepProtected => "защищённые пути",
        Label::StepDeny => "правила deny",
        Label::StepAllow => "правила allow",
        Label::StepGrant => "временные одобрения",
        Label::NoMatch => "нет совпадений",
        Label::Outcome => "Итог",
        Label::ExpansionPossible => "можно запросить расширение области",
        Label::ExpansionImpossible => "это нельзя открыть одобрением",
        Label::CurrentScope => "Разрешённая область",
        Label::Nothing => "нет",
        Label::Warnings => "Предупреждения",
        Label::Approved => "Утверждено",
        Label::UnapprovedEdits => "в файле политики есть неутверждённые правки",
        Label::Task => "Задача",
        Label::Usage => "Расход",
        Label::State => "Состояние",
        Label::LastChange => "Последнее изменение",
        Label::ChangedPaths => "изменённые пути",
        Label::Operations => "операции",
        Label::Moves => "перемещения",
        Label::RecoveryStorage => "хранилище восстановления",
        Label::Unfinished => "незавершённые",
        Label::NeedsComparing => "требуют сверки",
        Label::TemporaryApprovals => "временные одобрения",
        Label::PolicyFile => "файл политики",
        Label::History => "История",
        Label::Coverage => "Покрытие",
        Label::NotStarted => "задача не начата",
        Label::NoPolicyYet => "политика не утверждена",
        Label::Checks => "Проверки",
        Label::Passed => "ок",
        Label::Failed => "СБОЙ",
        Label::ChangedOutside => "Изменено в обход SafeScope",
        Label::NoBaseline => "нет базового состояния",
        Label::DriftAdded => "добавлено",
        Label::DriftModified => "изменено",
        Label::DriftRemoved => "удалено",
    }
}
