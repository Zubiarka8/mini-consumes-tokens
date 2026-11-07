pub const DIM: usize = 16;

pub fn bag_of_words(text: &str) -> Vec<f32> {
    let mut v = vec![0.0f32; DIM];
    for word in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        let h = word.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
            (h ^ b as u64).wrapping_mul(0x100_0000_01b3)
        });
        v[(h % DIM as u64) as usize] += 1.0;
    }
    v
}
