// CJK-aware text input — full implementation in Phase 5.
// CJK characters go via clipboard + Ctrl+V; Latin via direct key simulation.

/// Returns true if the text contains CJK (Chinese/Japanese/Korean) characters.
pub fn contains_cjk(text: &str) -> bool {
    text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)
        || ('\u{3040}'..='\u{309f}').contains(&c)
        || ('\u{30a0}'..='\u{30ff}').contains(&c))
}
