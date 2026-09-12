use super::{NumberFieldValue, clamp_number, commit_text_input};

#[test]
fn formats_integer_without_decimal() {
    assert_eq!(i32::format_value(&42), "42");
}

#[test]
fn formats_float_with_trimmed_fraction() {
    assert_eq!(f64::format_value(&1.5), "1.5");
    assert_eq!(f64::format_value(&2.0), "2");
}

#[test]
fn clamps_generic_values() {
    assert_eq!(clamp_number(10_i32, Some(0), Some(5)), 5);
    assert_eq!(clamp_number(1.5_f64, Some(2.0), Some(5.0)), 2.0);
}

#[test]
fn signed_integer_supports_intermediate_minus() {
    assert!(i32::is_intermediate_input("-"));
    assert!(!usize::is_intermediate_input("-"));
}

#[test]
fn float_supports_incomplete_exponent_intermediate() {
    assert!(f64::is_intermediate_input("1e"));
    assert!(f64::is_intermediate_input("1e-"));
    assert!(!f64::is_intermediate_input("1.5"));
}

#[test]
fn blur_commit_restores_current_value_for_intermediate_input() {
    assert_eq!(
        commit_text_input::<i32>("-", 7, Some(0), Some(10)),
        (7, "7".to_string())
    );
}

#[test]
fn blur_commit_clamps_and_formats_value() {
    assert_eq!(
        commit_text_input::<f64>("12.5", 0.0, Some(0.0), Some(10.0)),
        (10.0, "10".to_string())
    );
}
