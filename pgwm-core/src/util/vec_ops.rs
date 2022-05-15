use crate::error::Result;
use crate::push_heapless;

#[inline]
pub fn push_to_front<T, const N: usize>(target: &mut heapless::Vec<T, N>, item: T) -> Result<()> {
    push_heapless!(target, item)?;
    for i in (1..target.len()).rev() {
        target.swap(i, i - 1);
    }
    Ok(())
}

#[inline]
pub fn remove<T, const N: usize>(target: &mut heapless::Vec<T, N>, ind: usize) -> T {
    let prev_len = target.len();
    let out = target.swap_remove(ind);
    if prev_len > 2 {
        for i in ind..target.len() - 1 {
            target.swap(i, i + 1);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::push_to_front;
    use super::remove;

    #[test]
    fn push_to_front_test() {
        let mut heapless_vec: heapless::Vec<i32, 4> = heapless::Vec::new();
        let _ = heapless_vec.push(0);
        let _ = heapless_vec.push(1);
        let _ = heapless_vec.push(2);
        push_to_front(&mut heapless_vec, 3).unwrap();
        assert_eq!(3, heapless_vec[0]);
        assert_eq!(0, heapless_vec[1]);
        assert_eq!(1, heapless_vec[2]);
        assert_eq!(2, heapless_vec[3]);
    }

    #[test]
    fn remove_test() {
        let mut heapless_vec: heapless::Vec<i32, 4> = heapless::Vec::new();
        let _ = heapless_vec.push(0);
        let _ = heapless_vec.push(1);
        let _ = heapless_vec.push(2);
        let _ = heapless_vec.push(3);
        remove(&mut heapless_vec, 1);
        assert_eq!(0, heapless_vec[0]);
        assert_eq!(2, heapless_vec[1]);
        assert_eq!(3, heapless_vec[2]);
    }
}
