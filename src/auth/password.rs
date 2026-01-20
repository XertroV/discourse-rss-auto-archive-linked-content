use anyhow::{Context, Result};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

/// Hash a password using Argon2id.
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .context("Failed to hash password")?
        .to_string();

    Ok(password_hash)
}

/// Verify a password against its hash.
pub fn verify_password(password: &str, password_hash: &str) -> Result<bool> {
    let parsed_hash = PasswordHash::new(password_hash).context("Failed to parse password hash")?;

    let argon2 = Argon2::default();

    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// Validate password meets minimum length requirement.
/// Returns Ok(()) if valid, Err with message if invalid.
pub fn validate_password_strength(password: &str) -> Result<()> {
    const MIN_LENGTH: usize = 10;

    if password.len() < MIN_LENGTH {
        anyhow::bail!("Password must be at least {MIN_LENGTH} characters long");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hashing() {
        let password = "test_password_123!";
        let hash = hash_password(password).unwrap();

        assert!(verify_password(password, &hash).unwrap());
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_password_strength_validation() {
        // Valid passwords (10+ characters)
        assert!(validate_password_strength("abcdefghij").is_ok());
        assert!(validate_password_strength("1234567890").is_ok());
        assert!(validate_password_strength("MyP@ssw0rd123").is_ok());

        // Too short (< 10 characters)
        assert!(validate_password_strength("Short1!").is_err());
        assert!(validate_password_strength("123456789").is_err());
    }
}
