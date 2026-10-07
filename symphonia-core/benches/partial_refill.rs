// Run with: bash symphonia-core/benches/compare_partial_refill.sh
// Synthetic refill timings include cache reset and black_box overhead, not decoder throughput.

use criterion::{criterion_group, criterion_main};
use symphonia_core::{io, util};

// Access the real implementation's private state without adding a public benchmarking API.
#[allow(dead_code, unused_imports)]
mod bit {
    include!("../src/io/bit.rs");

    use std::hint::black_box;

    use criterion::{Bencher, Criterion, Throughput};

    use private::{FetchBitsLtr, FetchBitsRtl};

    const CURRENT: u8 = 0;
    const BYTE_LOOP: u8 = 1;
    const PADDED_COPY: u8 = 2;

    // Const parameters keep direction and implementation dispatch outside the timed hot path.
    #[inline]
    fn refill<const RTL: bool, const IMPLEMENTATION: u8>(
        buf: &[u8],
        mut bits: u64,
        mut n_bits_left: u32,
    ) -> (u64, u32, usize) {
        if IMPLEMENTATION == CURRENT {
            if RTL {
                let mut reader = BitReaderRtl { buf, bits, n_bits_left };
                reader.fetch_bits_partial().unwrap();
                return (reader.bits, reader.n_bits_left, reader.buf.len());
            }
            else {
                let mut reader = BitReaderLtr { buf, bits, n_bits_left };
                reader.fetch_bits_partial().unwrap();
                return (reader.bits, reader.n_bits_left, reader.buf.len());
            }
        }

        let num_bytes = (u64::BITS - n_bits_left) as usize >> 3;
        let mut num_bytes_read = 0;

        if IMPLEMENTATION == BYTE_LOOP {
            // Previous implementation from 51990467.
            for &byte in buf.iter().take(num_bytes) {
                bits |= u64::from(byte) << if RTL { n_bits_left } else { 56 - n_bits_left };
                n_bits_left += 8;
                num_bytes_read += 1;
            }
        }
        else {
            // Pre-51990467 implementation; guard the empty/full-cache case against shifting by 64.
            num_bytes_read = buf.len().min(num_bytes);
            if num_bytes_read > 0 {
                let mut padded = [0; 8];
                padded[..num_bytes_read].copy_from_slice(&buf[..num_bytes_read]);
                bits |= if RTL {
                    u64::from_le_bytes(padded) << n_bits_left
                }
                else {
                    u64::from_be_bytes(padded) >> n_bits_left
                };
                n_bits_left += (num_bytes_read as u32) << 3;
            }
        }

        (bits, n_bits_left, buf.len() - num_bytes_read)
    }

    fn cached_bits<const RTL: bool>(pattern: u64, n_bits_left: u32) -> u64 {
        if n_bits_left == 0 {
            0
        }
        else if RTL {
            pattern & (u64::MAX >> (64 - n_bits_left))
        }
        else {
            pattern & (u64::MAX << (64 - n_bits_left))
        }
    }

    fn bench_refills(
        b: &mut Bencher<'_>,
        cases: &[(&[u8], u64, u32)],
        refill: impl Fn(&[u8], u64, u32) -> (u64, u32, usize),
    ) {
        b.iter(|| {
            for &case in cases {
                let (buf, bits, n_bits_left) = black_box(case);
                black_box(refill(buf, bits, n_bits_left));
            }
        });
    }

    pub fn bench<const RTL: bool>(c: &mut Criterion) {
        let implementation = std::env::var("BIT_REFILL_IMPL").expect(
            "Run bash symphonia-core/benches/compare_partial_refill.sh to compare implementations",
        );
        assert!(
            matches!(implementation.as_str(), "current" | "byte_loop" | "padded_copy"),
            "BIT_REFILL_IMPL must be current, byte_loop, or padded_copy"
        );
        let input: [u8; 256] =
            std::array::from_fn(|i| (i as u8).wrapping_mul(37).wrapping_add(0x12));

        // Verify each implementation before timing, including tails and every cache occupancy.
        for pattern in [0, 0xa5a5_5a5a_f0f0_0f0f, u64::MAX] {
            for n_bits_left in 0..=64 {
                for len in 0..=input.len() {
                    let bits = cached_bits::<RTL>(pattern, n_bits_left);
                    let expected = refill::<RTL, BYTE_LOOP>(&input[..len], bits, n_bits_left);
                    assert_eq!(refill::<RTL, CURRENT>(&input[..len], bits, n_bits_left), expected);
                    assert_eq!(
                        refill::<RTL, PADDED_COPY>(&input[..len], bits, n_bits_left),
                        expected
                    );
                }
            }
        }

        let direction = if RTL { "rtl" } else { "ltr" };
        for (name, lengths, residuals) in [
            ("bulk/aligned", &[256][..], &[0, 8, 16, 24][..]),
            ("bulk/unaligned", &[256][..], &[1, 7, 9, 15, 17, 23, 25, 31][..]),
            ("eight-byte boundary", &[8][..], &[0, 1, 7, 8, 15, 31][..]),
            ("short tails", &[1, 2, 3, 4, 5, 6, 7][..], &[0, 1, 7, 8, 15, 31][..]),
            ("empty input", &[0][..], &[0, 1, 7, 8, 31, 64][..]),
            ("no whole-byte capacity", &[256][..], &[57, 58, 59, 60, 61, 62, 63, 64][..]),
        ] {
            let mut cases = Vec::new();
            for &len in lengths {
                for &n_bits_left in residuals {
                    let bits = cached_bits::<RTL>(0xa5a5_5a5a_f0f0_0f0f, n_bits_left);
                    cases.push((&input[..len], bits, n_bits_left));
                }
            }

            let mut group = c.benchmark_group(format!("{direction}/{name}"));
            // One timed iteration visits every state; throughput counts individual refills.
            group.throughput(Throughput::Elements(cases.len() as u64));
            // Matching IDs let Criterion compare different implementations via named baselines.
            group.bench_function("refill", |b| match implementation.as_str() {
                "current" => bench_refills(b, &cases, refill::<RTL, CURRENT>),
                "byte_loop" => bench_refills(b, &cases, refill::<RTL, BYTE_LOOP>),
                "padded_copy" => bench_refills(b, &cases, refill::<RTL, PADDED_COPY>),
                _ => unreachable!(),
            });
            group.finish();
        }
    }
}

criterion_group!(benches, bit::bench::<false>, bit::bench::<true>);
criterion_main!(benches);
