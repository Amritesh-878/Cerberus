//! API list prices per million tokens, for an estimate only; a model not here shows its tokens with no price.

use crate::tokens::Tokens;

/// Dollars per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rates {
    input: f64,
    output: f64,
    cache_read: f64,
}

struct Price {
    id: &'static str,
    name: &'static str,
    rates: Rates,
    /// Rates for a prompt over 100K tokens, where a model has them.
    long: Option<Rates>,
    /// Fast mode's input and output rates, where a model has them.
    fast: Option<(f64, f64)>,
}

/// A prompt over this many tokens is priced at `long`.
const LONG_PROMPT: u64 = 100_000;
/// 5-minute cache writes cost this times input; 1-hour writes cost `WRITE_1H`.
const WRITE_5M: f64 = 1.25;
const WRITE_1H: f64 = 2.0;

const fn rates(input: f64, output: f64, cache_read: f64) -> Rates {
    Rates {
        input,
        output,
        cache_read,
    }
}

// Anthropic's first-party list prices as of 2026-11; cache reads are 0.1x input unless the model says otherwise.
const PRICES: &[Price] = &[
    Price {
        id: "claude-fable-5-2",
        name: "Fable 5.2",
        rates: rates(8.0, 40.0, 0.20),
        long: None,
        fast: Some((16.0, 80.0)),
    },
    Price {
        id: "claude-mythos-5-2",
        name: "Mythos 5.2",
        rates: rates(8.0, 40.0, 0.20),
        long: None,
        fast: Some((16.0, 80.0)),
    },
    Price {
        id: "claude-fable-5-1",
        name: "Fable 5.1",
        rates: rates(10.0, 50.0, 0.25),
        long: None,
        fast: None,
    },
    Price {
        id: "claude-mythos-5-1",
        name: "Mythos 5.1",
        rates: rates(10.0, 50.0, 0.25),
        long: None,
        fast: None,
    },
    Price {
        id: "claude-fable-5",
        name: "Fable 5",
        rates: rates(10.0, 50.0, 1.00),
        long: None,
        fast: None,
    },
    Price {
        id: "claude-mythos-5",
        name: "Mythos 5",
        rates: rates(10.0, 50.0, 1.00),
        long: None,
        fast: None,
    },
    Price {
        id: "claude-opus-5-5",
        name: "Opus 5.5",
        rates: rates(4.0, 20.0, 0.20),
        long: None,
        fast: Some((8.0, 40.0)),
    },
    Price {
        id: "claude-opus-5",
        name: "Opus 5",
        rates: rates(5.0, 25.0, 0.50),
        long: None,
        fast: Some((10.0, 50.0)),
    },
    Price {
        id: "claude-opus-4-8",
        name: "Opus 4.8",
        rates: rates(5.0, 25.0, 0.50),
        long: None,
        fast: Some((10.0, 50.0)),
    },
    Price {
        id: "claude-opus-4-7",
        name: "Opus 4.7",
        rates: rates(5.0, 25.0, 0.50),
        long: None,
        fast: None,
    },
    Price {
        id: "claude-opus-4-6",
        name: "Opus 4.6",
        rates: rates(5.0, 25.0, 0.50),
        long: None,
        fast: None,
    },
    Price {
        id: "claude-sonnet-5-5",
        name: "Sonnet 5.5",
        rates: rates(2.0, 10.0, 0.10),
        long: None,
        fast: None,
    },
    Price {
        id: "claude-sonnet-5",
        name: "Sonnet 5",
        rates: rates(2.0, 10.0, 0.20),
        long: None,
        fast: None,
    },
    Price {
        id: "claude-sonnet-4-6",
        name: "Sonnet 4.6",
        rates: rates(3.0, 15.0, 0.30),
        long: None,
        fast: None,
    },
    Price {
        id: "claude-haiku-5-5",
        name: "Haiku 5.5",
        rates: rates(0.10, 0.50, 0.01),
        long: Some(rates(0.50, 2.50, 0.05)),
        fast: None,
    },
    Price {
        id: "claude-haiku-4-5",
        name: "Haiku 4.5",
        rates: rates(1.0, 5.0, 0.10),
        long: None,
        fast: None,
    },
];

/// The table's entry for a model id as a transcript records it, with or without a date suffix.
fn price_of(model: &str) -> Option<&'static Price> {
    PRICES.iter().find(|p| {
        model == p.id
            || model.strip_prefix(p.id).is_some_and(|rest| {
                rest.len() == 9
                    && rest.starts_with('-')
                    && rest[1..].bytes().all(|b| b.is_ascii_digit())
            })
    })
}

/// A short name for a model id, or the id itself when the table doesn't know it.
pub fn name_of(model: &str) -> String {
    price_of(model).map_or_else(|| model.to_owned(), |p| p.name.to_owned())
}

/// What one reply would cost at API prices, in dollars; `None` for a model or mode with no price here.
pub fn cost(model: &str, fast: bool, tokens: &Tokens, written_1h: u64) -> Option<f64> {
    let price = price_of(model)?;
    let prompt = tokens.input + tokens.cache_read + tokens.cache_write;
    let base = match price.long {
        Some(long) if prompt > LONG_PROMPT => long,
        _ => price.rates,
    };
    let rates = if fast {
        let (input, output) = price.fast?;
        // Fast mode raises every input rate alike, so cache reads keep their share of input.
        Rates {
            input,
            output,
            cache_read: base.cache_read * input / base.input,
        }
    } else {
        base
    };
    let written_1h = written_1h.min(tokens.cache_write);
    let written_5m = tokens.cache_write - written_1h;
    let per_million = |count: u64, rate: f64| count as f64 * rate / 1_000_000.0;
    Some(
        per_million(tokens.input, rates.input)
            + per_million(tokens.output, rates.output)
            + per_million(tokens.cache_read, rates.cache_read)
            + per_million(written_5m, rates.input * WRITE_5M)
            + per_million(written_1h, rates.input * WRITE_1H),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(input: u64, output: u64, cache_read: u64, cache_write: u64) -> Tokens {
        Tokens {
            input,
            output,
            cache_read,
            cache_write,
        }
    }

    fn near(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-9)
    }

    #[test]
    fn each_kind_of_token_has_its_own_rate() {
        // 1M of each on Opus 5.5: $4 in, $20 out, $0.20 read, $5 for 5-minute writes.
        let all = tokens(1_000_000, 1_000_000, 1_000_000, 1_000_000);

        assert!(near(cost("claude-opus-5-5", false, &all, 0), 29.2));
        assert!(near(
            cost("claude-opus-5-5", false, &all, 1_000_000),
            4.0 + 20.0 + 0.2 + 8.0
        ));
    }

    #[test]
    fn a_date_suffix_matches_but_a_longer_id_does_not() {
        assert_eq!(name_of("claude-haiku-4-5-20251001"), "Haiku 4.5");
        assert_eq!(name_of("claude-opus-5-5"), "Opus 5.5");
        assert_eq!(name_of("claude-opus-5"), "Opus 5");
        assert_eq!(
            name_of("claude-opus-4-5-20251101"),
            "claude-opus-4-5-20251101"
        );
        assert_eq!(cost("<synthetic>", false, &tokens(1, 1, 0, 0), 0), None);
    }

    #[test]
    fn haiku_5_5_prices_a_long_prompt_higher_and_fast_mode_only_where_offered() {
        let short = tokens(100_000, 1_000_000, 0, 0);
        let long = tokens(100_001, 1_000_000, 0, 0);

        assert!(near(cost("claude-haiku-5-5", false, &short, 0), 0.01 + 0.5));
        assert!(near(
            cost("claude-haiku-5-5", false, &long, 0),
            100_001.0 * 0.5 / 1e6 + 2.5
        ));
        assert!(near(
            cost(
                "claude-opus-5-5",
                true,
                &tokens(1_000_000, 0, 1_000_000, 0),
                0
            ),
            8.0 + 0.4
        ));
        assert_eq!(cost("claude-sonnet-5-5", true, &short, 0), None);
    }
}
