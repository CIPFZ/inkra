//! Renderer backend and adapter choice (SPEC 2.1 [回填 2026-10-13], D-1, D-4).
//!
//! The decisions are pure functions so they can be unit-tested; `main.rs` wires
//! them into eframe's wgpu setup.
//!
//! The adapter that worked last time is remembered in the internal preference
//! `general.graphics.lastAdapter`. wgpu 30 has no safe way to open a single DX12
//! adapter (every enumeration creates a device on each adapter to probe it), so the
//! cache cannot skip probing. It keeps the choice stable across starts and
//! remembers when DX12 offered only the WARP software adapter, so that the next
//! start goes to GL (keeping DX12 as the fallback) without probing twice.

use eframe::wgpu::{Backend, Backends, DeviceType};
use serde_json::{Value, json};

/// Preference key of the cache (not shown in the preferences window).
pub const PREF_KEY: &str = "general.graphics.lastAdapter";

/// Platform families with different default backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// DX12 by default, GL as the fallback for WARP-only machines.
    Windows,
    /// Metal.
    Mac,
    /// Vulkan or GL.
    Linux,
}

impl Os {
    /// The platform this binary was built for.
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Linux
        }
    }
}

/// What identifies an adapter across starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterKey {
    /// Adapter name as reported by the driver.
    pub name: String,
    /// PCI vendor id (0 when the backend does not report it).
    pub vendor: u32,
    /// PCI device id (0 when the backend does not report it).
    pub device: u32,
    /// Backend the adapter belongs to.
    pub backend: Backend,
    /// Hardware or software adapter.
    pub device_type: DeviceType,
}

impl AdapterKey {
    /// The key of a wgpu adapter.
    pub fn of(info: &eframe::wgpu::AdapterInfo) -> Self {
        Self {
            name: info.name.clone(),
            vendor: info.vendor,
            device: info.device,
            backend: info.backend,
            device_type: info.device_type,
        }
    }

    fn is_dx12_warp(&self) -> bool {
        self.backend == Backend::Dx12 && self.device_type == DeviceType::Cpu
    }
}

/// State of the automatic switch to GL when DX12 offers only WARP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlFallback {
    /// GL has not been tried.
    #[default]
    Untried,
    /// A start with GL first is under way (or crashed before its first frame).
    Pending,
    /// GL was unavailable or did not reach a first frame: stay on DX12.
    Failed,
}

/// The cached adapter (`general.graphics.lastAdapter`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cache {
    /// The adapter of the last start that presented a frame.
    pub adapter: AdapterKey,
    /// The GL fallback state (Windows only).
    pub gl: GlFallback,
}

fn backend_name(b: Backend) -> &'static str {
    match b {
        Backend::Vulkan => "vulkan",
        Backend::Metal => "metal",
        Backend::Dx12 => "dx12",
        Backend::Gl => "gl",
        Backend::BrowserWebGpu => "webgpu",
        Backend::Noop => "noop",
    }
}

fn backend_of(name: &str) -> Option<Backend> {
    [
        Backend::Vulkan,
        Backend::Metal,
        Backend::Dx12,
        Backend::Gl,
        Backend::BrowserWebGpu,
        Backend::Noop,
    ]
    .into_iter()
    .find(|b| backend_name(*b) == name)
}

fn type_name(t: DeviceType) -> &'static str {
    match t {
        DeviceType::IntegratedGpu => "integrated",
        DeviceType::DiscreteGpu => "discrete",
        DeviceType::VirtualGpu => "virtual",
        DeviceType::Cpu => "cpu",
        DeviceType::Other => "other",
    }
}

fn type_of(name: &str) -> Option<DeviceType> {
    [
        DeviceType::IntegratedGpu,
        DeviceType::DiscreteGpu,
        DeviceType::VirtualGpu,
        DeviceType::Cpu,
        DeviceType::Other,
    ]
    .into_iter()
    .find(|t| type_name(*t) == name)
}

impl Cache {
    /// Reads the preference value; anything malformed counts as no cache.
    pub fn from_json(v: &Value) -> Option<Self> {
        let o = v.as_object()?;
        let id = |k: &str| {
            o.get(k)
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
        };
        Some(Self {
            adapter: AdapterKey {
                name: o.get("name")?.as_str()?.to_owned(),
                vendor: id("vendor")?,
                device: id("device")?,
                backend: backend_of(o.get("backend")?.as_str()?)?,
                device_type: type_of(o.get("type")?.as_str()?)?,
            },
            gl: match o.get("glFallback").and_then(Value::as_str) {
                Some("pending") => GlFallback::Pending,
                Some("failed") => GlFallback::Failed,
                _ => GlFallback::Untried,
            },
        })
    }

    /// The preference value.
    pub fn to_json(&self) -> Value {
        let a = &self.adapter;
        let mut v = json!({
            "name": a.name,
            "vendor": a.vendor,
            "device": a.device,
            "backend": backend_name(a.backend),
            "type": type_name(a.device_type),
        });
        let gl = match self.gl {
            GlFallback::Untried => None,
            GlFallback::Pending => Some("pending"),
            GlFallback::Failed => Some("failed"),
        };
        if let Some(gl) = gl {
            v["glFallback"] = json!(gl);
        }
        v
    }
}

/// The backends to start with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Backends of the wgpu instance.
    pub backends: Backends,
    /// Why (for the `graphics:` log line).
    pub reason: &'static str,
    /// Cache value to store before starting (the GL fallback bookkeeping).
    pub store_first: Option<Cache>,
    /// Whether the adapter used is recorded after the first frame (not under
    /// `WGPU_BACKEND`, which is a diagnostic override).
    pub record: bool,
}

/// Chooses the backends from the `WGPU_BACKEND` override, the platform and the
/// cache.
pub fn plan(os: Os, env: Option<Backends>, cache: Option<&Cache>) -> Plan {
    let plan = |backends, reason| Plan {
        backends,
        reason,
        store_first: None,
        record: true,
    };
    if let Some(backends) = env {
        return Plan {
            record: false,
            ..plan(backends, "WGPU_BACKEND")
        };
    }
    match os {
        Os::Mac => plan(Backends::METAL, "default"),
        Os::Linux => plan(Backends::VULKAN | Backends::GL, "default"),
        Os::Windows => match cache {
            Some(c) if c.adapter.backend == Backend::Gl => plan(
                Backends::DX12 | Backends::GL,
                "cached: GL, DX12 offered only WARP",
            ),
            Some(c) if c.adapter.is_dx12_warp() => match c.gl {
                GlFallback::Untried => Plan {
                    store_first: Some(Cache {
                        gl: GlFallback::Pending,
                        ..c.clone()
                    }),
                    ..plan(
                        Backends::DX12 | Backends::GL,
                        "cached: DX12 offered only WARP, trying GL",
                    )
                },
                GlFallback::Pending => Plan {
                    store_first: Some(Cache {
                        gl: GlFallback::Failed,
                        ..c.clone()
                    }),
                    ..plan(
                        Backends::DX12,
                        "GL did not reach a first frame last time, staying on DX12",
                    )
                },
                GlFallback::Failed => plan(Backends::DX12, "DX12, GL unavailable"),
            },
            _ => plan(Backends::DX12, "default"),
        },
    }
}

/// An adapter wgpu found.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Its identity.
    pub key: AdapterKey,
    /// Whether it can present to the window.
    pub presents: bool,
}

/// Preference order of adapter kinds: hardware first, integrated before discrete
/// (power), software last.
fn type_rank(t: DeviceType) -> u8 {
    match t {
        DeviceType::IntegratedGpu => 0,
        DeviceType::DiscreteGpu => 1,
        DeviceType::VirtualGpu => 2,
        DeviceType::Other => 3,
        DeviceType::Cpu => 4,
    }
}

/// Picks an adapter: the cached one if it is a hardware adapter and still
/// present, otherwise the best kind, preferring the platform's primary backend
/// over GL among equals. Returns the index and whether the cache was hit.
pub fn pick(candidates: &[Candidate], cache: Option<&Cache>) -> Option<(usize, bool)> {
    let usable = || candidates.iter().enumerate().filter(|(_, c)| c.presents);
    if let Some(cached) = cache.map(|c| &c.adapter)
        && cached.device_type != DeviceType::Cpu
        && let Some((i, _)) = usable().find(|(_, c)| c.key == *cached)
    {
        return Some((i, true));
    }
    usable()
        .min_by_key(|(_, c)| {
            (
                type_rank(c.key.device_type),
                u8::from(c.key.backend == Backend::Gl),
            )
        })
        .map(|(i, _)| (i, false))
}

/// The cache to store after the first frame with `chosen`, if it changed.
/// `prev` is the cache as stored before this start.
pub fn updated(prev: Option<&Cache>, chosen: &AdapterKey) -> Option<Cache> {
    let gl = match prev.map(|c| c.gl) {
        // GL was tried and not chosen (no GL adapter), or failed before.
        Some(GlFallback::Pending | GlFallback::Failed) if chosen.is_dx12_warp() => {
            GlFallback::Failed
        }
        _ => GlFallback::Untried,
    };
    let next = Cache {
        adapter: chosen.clone(),
        gl,
    };
    (prev != Some(&next)).then_some(next)
}

/// One adapter for the log: `name (type, backend)`.
pub fn describe(key: &AdapterKey) -> String {
    format!("{} ({:?}, {:?})", key.name, key.device_type, key.backend)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str, backend: Backend, device_type: DeviceType) -> AdapterKey {
        AdapterKey {
            name: name.into(),
            vendor: 0x10de,
            device: 0x2206,
            backend,
            device_type,
        }
    }

    fn cand(name: &str, backend: Backend, device_type: DeviceType) -> Candidate {
        Candidate {
            key: key(name, backend, device_type),
            presents: true,
        }
    }

    fn cache(k: AdapterKey, gl: GlFallback) -> Cache {
        Cache { adapter: k, gl }
    }

    fn rtx() -> AdapterKey {
        key("RTX 3080", Backend::Dx12, DeviceType::DiscreteGpu)
    }

    fn warp() -> AdapterKey {
        key(
            "Microsoft Basic Render Driver",
            Backend::Dx12,
            DeviceType::Cpu,
        )
    }

    #[test]
    fn platform_defaults() {
        assert_eq!(plan(Os::Windows, None, None).backends, Backends::DX12);
        assert_eq!(plan(Os::Mac, None, None).backends, Backends::METAL);
        assert_eq!(
            plan(Os::Linux, None, None).backends,
            Backends::VULKAN | Backends::GL
        );
        assert!(plan(Os::Windows, None, None).record);
    }

    #[test]
    fn env_override_wins_and_is_not_recorded() {
        let c = cache(warp(), GlFallback::Untried);
        for os in [Os::Windows, Os::Mac, Os::Linux] {
            let p = plan(os, Some(Backends::GL), Some(&c));
            assert_eq!(p.backends, Backends::GL);
            assert_eq!(p.reason, "WGPU_BACKEND");
            assert!(!p.record);
            assert!(p.store_first.is_none());
        }
    }

    #[test]
    fn hardware_cache_keeps_dx12() {
        let c = cache(rtx(), GlFallback::Untried);
        let p = plan(Os::Windows, None, Some(&c));
        assert_eq!(p.backends, Backends::DX12);
        assert!(p.store_first.is_none());
    }

    #[test]
    fn warp_only_tries_gl_once() {
        // First start: DX12 offers only WARP, which is used and cached.
        let first = updated(None, &warp()).unwrap();
        assert_eq!(first.gl, GlFallback::Untried);
        // Second start: GL first, DX12 kept as the fallback; marked pending.
        let p = plan(Os::Windows, None, Some(&first));
        assert_eq!(p.backends, Backends::DX12 | Backends::GL);
        let pending = p.store_first.unwrap();
        assert_eq!(pending.gl, GlFallback::Pending);
        // GL worked: the GL adapter is cached and later starts keep GL first.
        let gl = key("RTX 3080", Backend::Gl, DeviceType::DiscreteGpu);
        let after = updated(Some(&pending), &gl).unwrap();
        assert_eq!(after.adapter.backend, Backend::Gl);
        assert_eq!(after.gl, GlFallback::Untried);
        assert_eq!(
            plan(Os::Windows, None, Some(&after)).backends,
            Backends::DX12 | Backends::GL
        );
    }

    #[test]
    fn gl_unavailable_is_not_retried() {
        let pending = cache(warp(), GlFallback::Pending);
        // No GL adapter: WARP was chosen again.
        let after = updated(Some(&pending), &warp()).unwrap();
        assert_eq!(after.gl, GlFallback::Failed);
        let p = plan(Os::Windows, None, Some(&after));
        assert_eq!(p.backends, Backends::DX12);
        assert!(p.store_first.is_none());
        assert_eq!(updated(Some(&after), &warp()), None);
    }

    #[test]
    fn gl_crash_before_first_frame_falls_back_to_dx12() {
        // The pending marker survived: the GL start never presented a frame.
        let pending = cache(warp(), GlFallback::Pending);
        let p = plan(Os::Windows, None, Some(&pending));
        assert_eq!(p.backends, Backends::DX12);
        assert_eq!(p.store_first.unwrap().gl, GlFallback::Failed);
    }

    #[test]
    fn new_gpu_resets_the_fallback() {
        let failed = cache(warp(), GlFallback::Failed);
        let after = updated(Some(&failed), &rtx()).unwrap();
        assert_eq!(after, cache(rtx(), GlFallback::Untried));
    }

    #[test]
    fn pick_prefers_hardware_then_primary_backend() {
        let c = [
            cand("WARP", Backend::Dx12, DeviceType::Cpu),
            cand("RTX GL", Backend::Gl, DeviceType::DiscreteGpu),
            cand("RTX", Backend::Dx12, DeviceType::DiscreteGpu),
        ];
        assert_eq!(pick(&c, None), Some((2, false)));
        // WARP on DX12 and a GPU on GL: GL.
        assert_eq!(pick(&c[..2], None), Some((1, false)));
        // Integrated before discrete.
        let c = [
            cand("dGPU", Backend::Vulkan, DeviceType::DiscreteGpu),
            cand("iGPU", Backend::Vulkan, DeviceType::IntegratedGpu),
        ];
        assert_eq!(pick(&c, None), Some((1, false)));
    }

    #[test]
    fn pick_uses_the_cache_hit() {
        let mut c = vec![
            cand("iGPU", Backend::Dx12, DeviceType::IntegratedGpu),
            Candidate {
                key: rtx(),
                presents: true,
            },
        ];
        let hit = cache(rtx(), GlFallback::Untried);
        assert_eq!(pick(&c, Some(&hit)), Some((1, true)));
        // Gone or unable to present: normal choice.
        c[1].presents = false;
        assert_eq!(pick(&c, Some(&hit)), Some((0, false)));
        // A cached software adapter never beats hardware.
        let c = [
            cand("WARP", Backend::Dx12, DeviceType::Cpu),
            cand("RTX GL", Backend::Gl, DeviceType::DiscreteGpu),
        ];
        let soft = cache(
            key("WARP", Backend::Dx12, DeviceType::Cpu),
            GlFallback::Pending,
        );
        assert_eq!(pick(&c, Some(&soft)), Some((1, false)));
        assert_eq!(pick(&[], None), None);
    }

    #[test]
    fn unchanged_cache_is_not_rewritten() {
        let c = cache(rtx(), GlFallback::Untried);
        assert_eq!(updated(Some(&c), &rtx()), None);
        assert!(updated(None, &rtx()).is_some());
    }

    #[test]
    fn json_round_trip_and_bad_values() {
        for c in [
            cache(rtx(), GlFallback::Untried),
            cache(warp(), GlFallback::Pending),
            cache(warp(), GlFallback::Failed),
        ] {
            assert_eq!(Cache::from_json(&c.to_json()), Some(c));
        }
        assert_eq!(Cache::from_json(&Value::Null), None);
        assert_eq!(Cache::from_json(&json!({"name": "x"})), None);
        let mut v = cache(rtx(), GlFallback::Untried).to_json();
        v["backend"] = json!("directx9");
        assert_eq!(Cache::from_json(&v), None);
    }
}
