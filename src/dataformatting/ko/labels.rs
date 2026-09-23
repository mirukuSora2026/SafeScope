//! The ko spellings of the fixed labels.
//!
//! Split from the messages beside them only for length. It is still one
//! exhaustive match on `Label`, so a new label still fails to compile until
//! every language has one.

use crate::dataformatting::Label;

pub(super) fn render(label: Label) -> &'static str {
    match label {
        Label::Allowed => "허용",
        Label::Refused => "거부",
        Label::NotCovered => "해당 없음",
        Label::Path => "경로",
        Label::Operation => "연산",
        Label::Policy => "정책",
        Label::PolicyVersion => "버전",
        Label::EvaluationSteps => "평가 과정",
        Label::StepProtected => "보호 경로",
        Label::StepDeny => "deny 규칙",
        Label::StepAllow => "allow 규칙",
        Label::StepGrant => "임시 승인",
        Label::NoMatch => "해당 없음",
        Label::Outcome => "결과",
        Label::ExpansionPossible => "범위 확장을 요청할 수 있습니다",
        Label::ExpansionImpossible => "승인으로 열 수 없습니다",
        Label::CurrentScope => "허용 범위",
        Label::Nothing => "없음",
        Label::Warnings => "경고",
        Label::Approved => "승인됨",
        Label::UnapprovedEdits => "정책 파일에 승인되지 않은 변경이 있습니다",
        Label::Task => "작업",
        Label::Usage => "사용량",
        Label::State => "상태",
        Label::LastChange => "마지막 변경",
        Label::ChangedPaths => "변경 경로",
        Label::Operations => "파일 작업",
        Label::Moves => "이동",
        Label::RecoveryStorage => "복구 저장소",
        Label::Unfinished => "미완료",
        Label::NeedsComparing => "대조 필요",
        Label::TemporaryApprovals => "임시 승인",
        Label::PolicyFile => "정책 파일",
        Label::History => "기록",
        Label::Coverage => "적용 범위",
        Label::NotStarted => "시작된 작업 없음",
        Label::NoPolicyYet => "승인된 정책 없음",
        Label::Checks => "점검",
        Label::Passed => "정상",
        Label::Failed => "실패",
        Label::ChangedOutside => "SafeScope 밖에서 바뀐 것",
        Label::NoBaseline => "기준 상태 없음",
        Label::DriftAdded => "추가됨",
        Label::DriftModified => "변경됨",
        Label::DriftRemoved => "삭제됨",
    }
}
