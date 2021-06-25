pub fn percentage(numerator: u64, denominator: u64) -> u64 {
    if denominator != 0 {
        (numerator * 100) / denominator
    } else {
        0
    }
}
