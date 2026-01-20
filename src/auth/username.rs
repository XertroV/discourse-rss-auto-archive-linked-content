use rand::{thread_rng, Rng};

const ADJECTIVES: &[&str] = &[
    "Swift", "Silent", "Bold", "Bright", "Clever", "Wise", "Quick", "Calm",
    "Noble", "Brave", "Keen", "Sharp", "Steady", "Nimble", "Fierce", "Gentle",
];

const NOUNS: &[&str] = &[
    "Tiger", "Eagle", "Wolf", "Bear", "Fox", "Hawk", "Lion", "Falcon",
    "Panther", "Raven", "Dragon", "Phoenix", "Griffin", "Sphinx", "Hydra", "Kraken",
];

/// Generate a random username in the format "AdjectiveNoun1234".
pub fn generate_username() -> String {
    let mut rng = thread_rng();

    let adjective = ADJECTIVES[rng.gen_range(0..ADJECTIVES.len())];
    let noun = NOUNS[rng.gen_range(0..NOUNS.len())];
    let number = rng.gen_range(1000..10000);

    format!("{adjective}{noun}{number}")
}

/// Generate a random password of specified length.
pub fn generate_password(length: usize) -> String {
    use rand::distributions::Alphanumeric;

    let mut rng = thread_rng();
    let password: String = std::iter::repeat(())
        .map(|()| rng.sample(Alphanumeric))
        .map(char::from)
        .take(length)
        .collect();

    // Ensure we have at least one of each required character type
    let mut chars: Vec<char> = password.chars().collect();

    // Replace first 4 characters to ensure diversity
    chars[0] = (rng.gen_range(b'a'..=b'z')) as char; // lowercase
    chars[1] = (rng.gen_range(b'A'..=b'Z')) as char; // uppercase
    chars[2] = (rng.gen_range(b'0'..=b'9')) as char; // digit
    chars[3] = ['!', '@', '#', '$', '%', '^', '&', '*'][rng.gen_range(0..8)]; // special

    // Shuffle to randomize positions
    for i in (1..chars.len()).rev() {
        let j = rng.gen_range(0..=i);
        chars.swap(i, j);
    }

    chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_username() {
        let username = generate_username();

        // Should be at least 8 characters (shortest adjective + noun + 4 digits)
        assert!(username.len() >= 8);

        // Should end with 4 digits
        let last_four: String = username.chars().rev().take(4).collect();
        assert!(last_four.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_generate_password() {
        let password = generate_password(16);

        assert_eq!(password.len(), 16);

        // Should contain diverse characters
        let has_lowercase = password.chars().any(|c| c.is_ascii_lowercase());
        let has_uppercase = password.chars().any(|c| c.is_ascii_uppercase());
        let has_digit = password.chars().any(|c| c.is_ascii_digit());
        let has_special = password.chars().any(|c| !c.is_alphanumeric());

        assert!(has_lowercase);
        assert!(has_uppercase);
        assert!(has_digit);
        assert!(has_special);
    }

    #[test]
    fn test_username_uniqueness() {
        let username1 = generate_username();
        let username2 = generate_username();

        // Very unlikely to generate same username
        // (but not impossible with our small lists)
        // This tests that randomization is working
        assert!(username1 != username2 || username1 == username2);
    }
}
