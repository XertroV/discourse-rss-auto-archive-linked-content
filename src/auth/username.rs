use anyhow::Result;
use rand::{thread_rng, Rng};
use sqlx::SqlitePool;

const ADJECTIVES: &[&str] = &[
    "Arcane", "Ancient", "Shadow", "Frost", "Fire", "Storm", "Iron", "Steel", "Golden", "Silver",
    "Dark", "Crimson", "Scarlet", "Jade", "Sacred", "Wild", "Grim", "Cursed", "Eldritch",
    "Spectral", "Runic", "Eternal", "Obsidian", "Bronze", "Oaken", "Stone", "Grey", "Black",
    "Ivory", "Azure", "Feral", "Primal",
];

const NOUNS: &[&str] = &[
    "Dragon",
    "Wyrm",
    "Basilisk",
    "Hydra",
    "Sphinx",
    "Chimera",
    "Manticore",
    "Behemoth",
    "Wizard",
    "Ranger",
    "Druid",
    "Cleric",
    "Paladin",
    "Thief",
    "Barbarian",
    "Monk",
    "Knight",
    "Sorcerer",
    "Bard",
    "Giant",
    "Golem",
    "Lich",
    "Demon",
    "Wraith",
    "Owlbear",
    "Beholder",
    "Cockatrice",
    "Minotaur",
    "Tarrasque",
    "Troll",
    "Orc",
    "Kobold",
];

/// Generate a random username in the format "AdjectiveNoun1234".
pub fn generate_username() -> String {
    let mut rng = thread_rng();

    let adjective = ADJECTIVES[rng.gen_range(0..ADJECTIVES.len())];
    let noun = NOUNS[rng.gen_range(0..NOUNS.len())];
    let number = rng.gen_range(1000..10000);

    format!("{adjective}{noun}{number}")
}

/// Generate a unique username by checking against the database.
/// Retries up to 10 times, then falls back to appending more random digits.
pub async fn generate_unique_username(pool: &SqlitePool) -> Result<String> {
    for _ in 0..10 {
        let username = generate_username();
        if !crate::db::username_exists(pool, &username).await? {
            return Ok(username);
        }
    }

    // Fallback: append more random digits to make it unique
    let mut rng = thread_rng();
    let base = generate_username();
    let extra = rng.gen_range(10000..100000);
    Ok(format!("{base}{extra}"))
}

/// Validate a display name.
/// Rules: 1-20 characters, no spaces.
/// Returns Ok(()) if valid, Err with message if invalid.
pub fn validate_display_name(display_name: &str) -> Result<()> {
    if display_name.is_empty() {
        anyhow::bail!("Display name cannot be empty");
    }

    if display_name.len() > 20 {
        anyhow::bail!("Display name must be at most 20 characters");
    }

    if display_name.contains(' ') {
        anyhow::bail!("Display name cannot contain spaces");
    }

    Ok(())
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
        use std::collections::HashSet;

        // Generate many usernames and check for collisions
        let mut seen = HashSet::new();
        let iterations = 1000;

        for _ in 0..iterations {
            let username = generate_username();
            seen.insert(username);
        }

        // With 32*32*9000 = 9,216,000 possibilities,
        // 1000 generations should have near-zero collision probability.
        // Allow at most 1 collision (birthday paradox threshold is ~1500 for 50% chance).
        assert!(
            seen.len() >= iterations - 1,
            "Too many collisions: generated {} unique usernames out of {}",
            seen.len(),
            iterations
        );
    }
}
