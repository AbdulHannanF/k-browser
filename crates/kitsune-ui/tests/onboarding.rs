use kitsune_ui::app::BudgetState;

#[test]
fn default_budget_is_100_actions() {
    let budget = BudgetState::default();
    assert_eq!(budget.total, 100);
    assert_eq!(budget.used, 0);
    assert!((budget.fraction() - 0.0).abs() < f32::EPSILON);
}
