# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0] - 2026-11-17

### Added
- **4D Cartesian Kerr-Schild Geodesic Raymarcher**: RK4 Hamiltonian integration for null geodesics ($\mathcal{H} = 0$), with real-time toggle to Schwarzschild (`[K]`).
- **Relativistic Accretion Disk**: Keplerian differential rotation, polar plasma turbulence, blackbody emission, and toggleable Physical vs. Cinematic Doppler beaming (`[V]`).
- **Exact Relativistic Telemetry**: ZAMO lapse time dilation ($\alpha$), Bardeen-Press-Teukolsky ISCO, frame-dragging near the ergosphere, and live `You` vs `Earth` dual clocks.
- **Lorentz Aberration & Gravitational Blueshift**: Real-time Special Relativistic aberration (`aberrate_ray`) and blueshift when diving close to the horizon.
- **Hawking Evaporation & Time Rewind (`[E]`)**: Accelerated cosmological warp ($M(\tau) = (1 - \tau)^{1/3}$), blackbody horizon heating, spherical gamma-ray shockwave, and automatic time rewind.
- **Dual-Mode Terminal Rendering**: 24-bit TrueColor `Half-Block` (`▀`) and classic `ASCII` (`[M]`), with separable 2D HDR Bloom, ACES Filmic Tone Mapping, and DEC Mode 2026 synchronized output.
- **Cross-Platform Auto-Localization (PT/EN)**: Automatic OS locale detection across Windows, macOS, and Linux (`--lang pt|en` override).
- **CI/CD & Benchmarks**: GitHub Actions CI + 6-target release pipeline and `examples/bench.rs`.
