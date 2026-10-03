pub fn greet(name: &str) -> String {
    format!("Hello, {}?", name)
}

#[cfg(test)]
mod tests {
    use super::greet;

    #[test]
    fn greet_uses_exclamation_mark() {
        assert_eq!(greet("Ada"), "Hello, Ada!");
    }
}
