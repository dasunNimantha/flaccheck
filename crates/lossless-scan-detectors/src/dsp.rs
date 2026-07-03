use realfft::RealFftPlanner;
use rustfft::num_complex::Complex;

/// Welch PSD estimate for mono signal. Returns (frequencies_hz, power_linear).
pub fn welch_psd(samples: &[f32], sample_rate: u32, fft_size: usize) -> (Vec<f64>, Vec<f64>) {
  if samples.len() < fft_size {
    return (vec![], vec![]);
  }

  let hop = fft_size / 2;
  let mut planner = RealFftPlanner::<f32>::new();
  let r2c = planner.plan_fft_forward(fft_size);
  let mut scratch = r2c.make_scratch_vec();
  let mut spectrum = r2c.make_output_vec();
  let window: Vec<f32> = (0..fft_size)
    .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (fft_size - 1) as f32).cos()))
    .collect();

  let mut accum = vec![0.0f64; fft_size / 2 + 1];
  let mut count = 0usize;

  let mut start = 0;
  while start + fft_size <= samples.len() {
    let mut input: Vec<f32> = samples[start..start + fft_size]
      .iter()
      .zip(window.iter())
      .map(|(s, w)| s * w)
      .collect();
    r2c.process_with_scratch(&mut input, &mut spectrum, &mut scratch)
      .expect("fft");

    for (i, c) in spectrum.iter().enumerate() {
      accum[i] += (c.re * c.re + c.im * c.im) as f64;
    }
    count += 1;
    start += hop;
  }

  if count == 0 {
    return (vec![], vec![]);
  }

  let bin_hz = sample_rate as f64 / fft_size as f64;
  let freqs: Vec<f64> = (0..accum.len()).map(|i| i as f64 * bin_hz).collect();
  let psd: Vec<f64> = accum.iter().map(|v| v / count as f64).collect();
  (freqs, psd)
}

/// Average PSD across multiple windows.
pub fn average_psd(windows: &[&[f32]], sample_rate: u32, fft_size: usize) -> (Vec<f64>, Vec<f64>) {
  let mut merged_freqs = Vec::new();
  let mut merged_psd: Vec<f64> = vec![];

  for (i, w) in windows.iter().enumerate() {
    let (f, p) = welch_psd(w, sample_rate, fft_size);
    if f.is_empty() {
      continue;
    }
    if i == 0 {
      merged_freqs = f;
      merged_psd = p;
    } else {
      for (a, b) in merged_psd.iter_mut().zip(p.iter()) {
        *a += b;
      }
    }
  }

  if !merged_psd.is_empty() {
    let n = windows.len() as f64;
    for v in &mut merged_psd {
      *v /= n;
    }
  }

  (merged_freqs, merged_psd)
}

pub fn db_from_power(p: f64) -> f64 {
  10.0 * (p.max(1e-20)).log10()
}

/// Hilbert transform via FFT (analytic signal).
pub fn hilbert_envelope(samples: &[f32]) -> Vec<f32> {
  let n = samples.len();
  if n < 4 {
    return samples.to_vec();
  }
  let fft_size = n.next_power_of_two();
  let mut planner = rustfft::FftPlanner::new();
  let fft = planner.plan_fft_forward(fft_size);
  let ifft = planner.plan_fft_inverse(fft_size);

  let mut buffer: Vec<Complex<f32>> = samples
    .iter()
    .map(|&s| Complex::new(s, 0.0))
    .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
    .take(fft_size)
    .collect();

  fft.process(&mut buffer);

  let half = fft_size / 2;
  for i in 1..half {
    buffer[i] *= 2.0;
  }
  for i in half + 1..fft_size {
    buffer[i] = Complex::new(0.0, 0.0);
  }

  ifft.process(&mut buffer);
  buffer
    .iter()
    .take(n)
    .map(|c| (c.re * c.re + c.im * c.im).sqrt())
    .collect()
}
