use rand::{distributions::Alphanumeric, thread_rng, Rng};

/// Generate a CSRF token for form protection.
pub fn generate_csrf_token() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_csrf_token() {
        let token1 = generate_csrf_token();
        let token2 = generate_csrf_token();

        assert_eq!(token1.len(), 32);
        assert_eq!(token2.len(), 32);
        assert_ne!(token1, token2);
        assert!(token1.chars().all(|c| c.is_alphanumeric()));
    }
}
