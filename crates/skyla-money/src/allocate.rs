use crate::{Money, MoneyError};

/// Splits `total` into parts proportional to `weights`, with the parts summing
/// to `total` exactly.
///
/// Each part first gets its proportional share rounded toward zero. The
/// leftover minor units (always fewer than the number of parts) go one each
/// to the parts with the largest discarded remainders, ties broken by larger
/// weight and then by earlier position. The result is deterministic. A zero
/// weight always gets zero. Negative totals split the same way, with every
/// part negative.
pub fn allocate(total: Money, weights: &[u64]) -> Result<Vec<Money>, MoneyError> {
    let weight_sum: u128 = weights.iter().map(|&w| u128::from(w)).sum();
    if weight_sum == 0 {
        return Err(MoneyError::EmptyAllocation);
    }
    let abs = u128::from(total.minor().unsigned_abs());

    let mut parts: Vec<u128> = Vec::with_capacity(weights.len());
    let mut remainders: Vec<(u128, u64, usize)> = Vec::with_capacity(weights.len());
    for (index, &weight) in weights.iter().enumerate() {
        // abs < 2^64 and weight < 2^64, so the product fits in u128.
        let scaled = abs * u128::from(weight);
        parts.push(scaled / weight_sum);
        remainders.push((scaled % weight_sum, weight, index));
    }

    let assigned: u128 = parts.iter().sum();
    let leftover = usize::try_from(abs - assigned).map_err(|_| MoneyError::Overflow)?;
    remainders.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    for &(_, _, index) in remainders.iter().take(leftover) {
        parts[index] += 1;
    }

    let negative = total.is_negative();
    parts
        .into_iter()
        .map(|part| {
            let signed = i128::try_from(part).map_err(|_| MoneyError::Overflow)?;
            let signed = if negative { -signed } else { signed };
            i64::try_from(signed)
                .map(|minor| Money::new(minor, total.currency()))
                .map_err(|_| MoneyError::Overflow)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Currency;
    use proptest::prelude::*;

    fn czk(minor: i64) -> Money {
        Money::new(minor, Currency::CZK)
    }

    #[test]
    fn splits_a_crown_three_ways_exactly() {
        let parts = allocate(czk(100), &[1, 1, 1]).unwrap();
        assert_eq!(parts, vec![czk(34), czk(33), czk(33)]);
    }

    #[test]
    fn gives_leftovers_to_the_largest_remainders() {
        // 10 split 1:2:3 → exact 1.67, 3.33, 5.0 → 2, 3, 5.
        assert_eq!(
            allocate(czk(10), &[1, 2, 3]).unwrap(),
            vec![czk(2), czk(3), czk(5)]
        );
    }

    #[test]
    fn handles_negative_totals_and_zero_weights() {
        assert_eq!(
            allocate(czk(-100), &[1, 0, 1]).unwrap(),
            vec![czk(-50), czk(0), czk(-50)]
        );
        assert_eq!(allocate(czk(5), &[0, 3]).unwrap(), vec![czk(0), czk(5)]);
    }

    #[test]
    fn rejects_empty_or_all_zero_weights() {
        assert_eq!(allocate(czk(1), &[]), Err(MoneyError::EmptyAllocation));
        assert_eq!(allocate(czk(1), &[0, 0]), Err(MoneyError::EmptyAllocation));
    }

    #[test]
    fn handles_the_extremes() {
        let parts = allocate(czk(i64::MIN), &[1, 1]).unwrap();
        assert_eq!(Money::sum(Currency::CZK, parts).unwrap(), czk(i64::MIN));
        let parts = allocate(czk(i64::MAX), &[u64::MAX, 1]).unwrap();
        assert_eq!(Money::sum(Currency::CZK, parts).unwrap(), czk(i64::MAX));
    }

    proptest! {
        #[test]
        fn parts_always_sum_to_the_total(total: i64, weights in prop::collection::vec(0_u64..1_000_000, 1..12)) {
            prop_assume!(weights.iter().any(|&w| w > 0));
            let parts = allocate(czk(total), &weights).unwrap();
            prop_assert_eq!(parts.len(), weights.len());
            prop_assert_eq!(Money::sum(Currency::CZK, parts.iter().copied()).unwrap(), czk(total));
        }

        #[test]
        fn each_part_is_within_one_unit_of_its_exact_share(total in -10_000_000_000_i64..10_000_000_000, weights in prop::collection::vec(0_u64..10_000, 1..8)) {
            let sum: u64 = weights.iter().sum();
            prop_assume!(sum > 0);
            let parts = allocate(czk(total), &weights).unwrap();
            for (part, &w) in parts.iter().zip(&weights) {
                // |part × sum − total × w| < sum  ⇔  |part − exact share| < 1
                let lhs = i128::from(part.minor()) * i128::from(sum) - i128::from(total) * i128::from(w);
                prop_assert!(lhs.abs() < i128::from(sum));
                if w == 0 { prop_assert_eq!(part.minor(), 0); }
            }
        }
    }
}
