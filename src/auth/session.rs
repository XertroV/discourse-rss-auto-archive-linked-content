use rand::{distributions::Alphanumeric, thread_rng, Rng};

/// Generate a cryptographically secure random session token.
pub fn generate_session_token() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(64)
        .map(char::from)
        .collect()
}

/// Session duration in seconds.
pub enum SessionDuration {
    /// 1 hour for non-remember-me sessions
    Short,
    /// 30 days for remember-me sessions
    Long,
}

impl SessionDuration {
    #[must_use]
    pub const fn as_seconds(&self) -> i64 {
        match self {
            Self::Short => 3600,     // 1 hour
            Self::Long => 2_592_000, // 30 days
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_session_token() {
        let token1 = generate_session_token();
        let token2 = generate_session_token();

        assert_eq!(token1.len(), 64);
        assert_eq!(token2.len(), 64);
        assert_ne!(token1, token2); // Should be unique
        assert!(token1.chars().all(|c| c.is_alphanumeric()));
    }

    #[test]
    fn test_session_duration() {
        assert_eq!(SessionDuration::Short.as_seconds(), 3600);
        assert_eq!(SessionDuration::Long.as_seconds(), 2_592_000);
    }
}
