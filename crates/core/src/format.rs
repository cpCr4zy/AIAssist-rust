//! How catalogue values are shown in the model picker.

use crate::ModelInfo;

/// A per-token price as dollars per million tokens: `$2.00/M`, `Free`, or empty when unknown.
pub fn format_price(per_token: Option<f64>) -> String {
    match per_token {
        None => String::new(),
        Some(p) if !p.is_finite() => String::new(),
        Some(p) if p <= 0.0 => "Free".to_owned(),
        Some(p) => format!("${:.2}/M", p * 1_000_000.0),
    }
}

/// A compact context length: `1M`, `1.1M`, `128K`, `8000 → 8K`, `512`; empty when unknown or zero.
pub fn format_context(length: Option<u64>) -> String {
    match length {
        None | Some(0) => String::new(),
        Some(n) if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1_000_000.0).replace(".0M", "M"),
        Some(n) if n >= 1_000 => format!("{}K", n / 1_000),
        Some(n) => n.to_string(),
    }
}

/// `Name (vendor/model-id)`, or the id twice when the name is missing.
pub fn model_label(model: &ModelInfo) -> String {
    let name = if model.name.trim().is_empty() { &model.id } else { &model.name };
    format!("{name} ({})", model.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn prices() {
        assert_eq!(format_price(None), "");
        assert_eq!(format_price(Some(0.0)), "Free");
        assert_eq!(format_price(Some(-1.0)), "Free");
        assert_eq!(format_price(Some(0.000002)), "$2.00/M");
        assert_eq!(format_price(Some(0.00000015)), "$0.15/M");
        assert_eq!(format_price(Some(f64::NAN)), "");
    }

    #[test]
    fn context_lengths() {
        assert_eq!(format_context(None), "");
        assert_eq!(format_context(Some(0)), "");
        assert_eq!(format_context(Some(512)), "512");
        assert_eq!(format_context(Some(8_000)), "8K");
        assert_eq!(format_context(Some(128_000)), "128K");
        assert_eq!(format_context(Some(1_000_000)), "1M");
        assert_eq!(format_context(Some(1_048_576)), "1M");
        assert_eq!(format_context(Some(1_100_000)), "1.1M");
    }

    #[test]
    fn labels() {
        let m = ModelInfo { id: "a/b".into(), name: "B".into(), ..Default::default() };
        assert_eq!(model_label(&m), "B (a/b)");
        let m = ModelInfo { id: "a/b".into(), ..Default::default() };
        assert_eq!(model_label(&m), "a/b (a/b)");
    }

    proptest! {
        #[test]
        fn formatting_never_panics(p in any::<f64>(), n in any::<u64>()) {
            let _ = format_price(Some(p));
            let _ = format_context(Some(n));
        }
    }
}
