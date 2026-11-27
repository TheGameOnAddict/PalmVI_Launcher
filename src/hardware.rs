use crate::theme;
use eframe::egui::{self, Color32, RichText, Ui};
use serde::Deserialize;
use std::sync::{mpsc, OnceLock};

#[derive(Deserialize)]
struct Reference {
    ps5_gpu_tflops: f32,
    ps5_cpu_tflops: f32,
    gpus: Vec<Rating>,
    cpus: Vec<Rating>,
}

#[derive(Deserialize)]
struct Rating {
    models: Vec<String>,
    tflops: f32,
    #[serde(default)]
    memory_gib: Vec<u64>,
}

impl Reference {
    // Both meters share a linear scale that stays fixed when the selected GPU changes.
    fn meter_limit(&self) -> f32 {
        self.gpus
            .iter()
            .map(|r| r.tflops / self.ps5_gpu_tflops)
            .chain(self.cpus.iter().map(|r| r.tflops / self.ps5_cpu_tflops))
            .fold(2.0, f32::max)
            .ceil()
    }
}

fn reference() -> &'static Reference {
    static DATA: OnceLock<Reference> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../data/hardware-comparison.json"))
            .expect("valid bundled hardware comparison")
    })
}

#[derive(Debug, Default)]
struct Machine {
    cpu: Option<String>,
    ram_kib: Option<u64>,
    gpus: Vec<Adapter>,
    desktop: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct Adapter {
    name: String,
    vram_bytes: u64,
    vendor: u32,
}

fn model_key(name: &str) -> String {
    let clean = name
        .to_lowercase()
        .replace("(r)", "")
        .replace("(tm)", "")
        .replace(['®', '™'], "");
    let name = clean.split('@').next().unwrap_or(&clean);
    let mut words = Vec::new();
    for word in name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        if matches!(word, "w" | "with" | "processor") {
            break;
        }
        if word == "core"
            && words
                .last()
                .is_some_and(|v: &&str| v.parse::<u32>().is_ok())
        {
            words.pop();
            break;
        }
        if word == "gen"
            && words.last().is_some_and(|v| {
                v.trim_end_matches(['s', 't', 'n', 'd', 'r', 'h'])
                    .parse::<u32>()
                    .is_ok()
            })
        {
            words.pop();
            continue;
        }
        if !matches!(word, "nvidia" | "amd" | "intel" | "gpu" | "graphics") {
            words.push(word);
        }
    }
    words.join(" ")
}

fn rating<'a>(rows: &'a [Rating], name: &str, memory_gib: Option<u64>) -> Option<&'a Rating> {
    let key = model_key(name);
    rows.iter().find(|r| {
        (r.memory_gib.is_empty() || memory_gib.is_some_and(|gib| r.memory_gib.contains(&gib)))
            && r.models.iter().any(|m| model_key(m) == key)
    })
}

#[cfg(windows)]
fn detect() -> Machine {
    use windows::core::w;
    use windows::Win32::{
        Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE},
        System::{
            Power::{
                PlatformRoleDesktop, PlatformRoleEnterpriseServer, PlatformRolePerformanceServer,
                PlatformRoleSOHOServer, PlatformRoleWorkstation, PowerDeterminePlatformRoleEx,
                POWER_PLATFORM_ROLE_V2,
            },
            Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ},
            SystemInformation::{
                GetPhysicallyInstalledSystemMemory, GlobalMemoryStatusEx, MEMORYSTATUSEX,
            },
        },
    };
    let mut machine = Machine::default();
    // All Win32 output buffers are owned here. DXGI's COM wrappers release each adapter and factory.
    unsafe {
        let mut name = [0u16; 256];
        let mut bytes = std::mem::size_of_val(&name) as u32;
        if RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0"),
            w!("ProcessorNameString"),
            RRF_RT_REG_SZ,
            None,
            Some(name.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
        .is_ok()
        {
            machine.cpu = Some(wide_name(&name));
        }
        let mut kib = 0;
        if GetPhysicallyInstalledSystemMemory(&mut kib).is_ok() && kib > 0 {
            machine.ram_kib = Some(kib);
        } else {
            let mut status = MEMORYSTATUSEX {
                dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
                ..Default::default()
            };
            if GlobalMemoryStatusEx(&mut status).is_ok() {
                machine.ram_kib = Some(status.ullTotalPhys / 1024);
            }
        }
        let role = PowerDeterminePlatformRoleEx(POWER_PLATFORM_ROLE_V2);
        machine.desktop = [
            PlatformRoleDesktop,
            PlatformRoleWorkstation,
            PlatformRoleSOHOServer,
            PlatformRoleEnterpriseServer,
            PlatformRolePerformanceServer,
        ]
        .contains(&role);
        if let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() {
            let mut i = 0;
            while let Ok(adapter) = factory.EnumAdapters1(i) {
                i += 1;
                if let Ok(desc) = adapter.GetDesc1() {
                    if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0 {
                        let gpu = Adapter {
                            name: wide_name(&desc.Description),
                            vram_bytes: desc.DedicatedVideoMemory as u64,
                            vendor: desc.VendorId,
                        };
                        // Multiple DXGI nodes can expose indistinguishable comparison choices.
                        if !machine.gpus.contains(&gpu) {
                            machine.gpus.push(gpu);
                        }
                    }
                }
            }
        }
    }
    machine
        .gpus
        .sort_by_key(|gpu| std::cmp::Reverse(gpu.vram_bytes));
    machine
}

#[cfg(windows)]
fn wide_name(name: &[u16]) -> String {
    String::from_utf16_lossy(&name[..name.iter().position(|c| *c == 0).unwrap_or(name.len())])
        .trim()
        .to_owned()
}

#[cfg(not(windows))]
fn detect() -> Machine {
    Machine::default()
}

#[derive(Default)]
pub struct Panel {
    request: Option<mpsc::Receiver<Machine>>,
    machine: Option<Machine>,
    selected: usize,
}

impl Panel {
    fn poll(&mut self, ui: &mut Ui) {
        if self.machine.is_none() && self.request.is_none() {
            let (tx, rx) = mpsc::channel();
            self.request = Some(rx);
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let _ = tx.send(detect());
                ctx.request_repaint();
            });
        }
        if let Some(request) = &self.request {
            match request.try_recv() {
                Ok(machine) => {
                    self.machine = Some(machine);
                    self.request = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.machine = Some(Machine::default());
                    self.request = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    pub fn header(&mut self,ui:&mut Ui)->bool{
        self.poll(ui);
        let data=reference();
        let machine=self.machine.as_ref();
        let cpu=machine.and_then(|m|m.cpu.as_deref()).and_then(|n|rating(&data.cpus,n,None)).map(|r|r.tflops/data.ps5_cpu_tflops);
        let gpu=machine.filter(|m|m.desktop).and_then(|m|m.gpus.get(self.selected)).and_then(|g|rating(&data.gpus,&g.name,Some((g.vram_bytes as f64/1073741824.0).round() as u64))).map(|r|r.tflops/data.ps5_gpu_tflops);
        let (rect,response)=ui.allocate_exact_size(egui::vec2(221.0,42.0),egui::Sense::click());
        let p=ui.painter();let left=rect.left()+8.0;
        if response.hovered() || response.has_focus() {
            p.rect(rect, 7.0, theme::raised(), egui::Stroke::new(1.0_f32, theme::lilac()));
        }
        for (i,(label,ratio,color)) in [("CPU",cpu,theme::good()),("GPU",gpu,egui::Color32::from_rgb(197,179,240))].into_iter().enumerate(){
            let y=rect.center().y-8.0+i as f32*16.0;
            p.text(egui::pos2(left,y),egui::Align2::LEFT_CENTER,label,theme::strong(10.5),theme::muted());
            let bar=egui::Rect::from_min_size(egui::pos2(left+28.0,y-2.5),egui::vec2(100.0,5.0));
            p.rect_filled(bar,2.5,theme::raised());
            if let Some(v)=ratio{p.rect_filled(egui::Rect::from_min_size(bar.min,egui::vec2(bar.width()*(v/data.meter_limit()).clamp(0.0,1.0),5.0)),2.5,color);}
            let x=bar.left()+bar.width()/data.meter_limit();
            p.line_segment([egui::pos2(x,y-3.5),egui::pos2(x,y+3.5)],egui::Stroke::new(1.0_f32,theme::muted()));
            p.text(egui::pos2(left+168.0,y),egui::Align2::RIGHT_CENTER,ratio.map_or_else(||"—".into(),|v|format!("{v:.2}×")),theme::strong(11.0),theme::white());
        }
        p.text(egui::pos2(left+174.0,rect.center().y),egui::Align2::LEFT_CENTER,"vs PS5",theme::regular(11.0),theme::muted());
        response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Ticks mark PS5 compute at 1×. Click for hardware details.").clicked()
    }

    pub fn show(&mut self, ui: &mut Ui) {
        self.poll(ui);
        theme::card(ui, 18.0, |ui| {
            theme::title(ui, "Your system", 27.0, ui.available_width());
            ui.add_space(10.0);
            let Some(machine) = &self.machine else {
                ui.spinner();
                text(ui, "Detecting hardware…", theme::muted());
                return;
            };
            let data = reference();
            theme::caps(ui, "Graphics", 11.0, theme::lilac());
            if machine.gpus.len() > 1 {
                egui::ComboBox::from_id_salt("comparison-adapter")
                    .width(ui.available_width())
                    .truncate()
                    .selected_text(&machine.gpus[self.selected].name)
                    .show_ui(ui, |ui| {
                        for (i, gpu) in machine.gpus.iter().enumerate() {
                            ui.selectable_value(&mut self.selected, i, &gpu.name);
                        }
                    })
                    .response
                    .on_hover_text("Choose the GPU used for this comparison.");
            } else {
                text(
                    ui,
                    machine
                        .gpus
                        .first()
                        .map_or("Unavailable", |g| g.name.as_str()),
                    theme::white(),
                );
            }
            let gpu = machine.gpus.get(self.selected);
            if let Some(gpu) = gpu.filter(|gpu| gpu.vram_bytes > 0) {
                text(
                    ui,
                    &format!("{:.1} GiB VRAM", gpu.vram_bytes as f64 / 1073741824.0),
                    theme::muted(),
                );
            }
            ui.add_space(12.0);
            theme::caps(ui, "Processor", 11.0, theme::lilac());
            text(
                ui,
                machine.cpu.as_deref().unwrap_or("Unavailable"),
                theme::white(),
            );
            text(
                ui,
                &machine.ram_kib.map_or_else(
                    || "System memory unavailable".into(),
                    |kib| format!("{:.1} GiB RAM", kib as f64 / 1048576.0),
                ),
                theme::muted(),
            );
            section_break(ui);
            theme::caps(ui, "Compute performance", 11.0, theme::pink());
            text(ui, "PS5 reference  ·  1×", theme::muted());
            ui.add_space(6.0);
            let graphics = gpu.filter(|_| machine.desktop).and_then(|g| {
                rating(
                    &data.gpus,
                    &g.name,
                    Some((g.vram_bytes as f64 / 1073741824.0).round() as u64),
                )
            });
            let cpu = machine
                .cpu
                .as_deref()
                .and_then(|name| rating(&data.cpus, name, None));
            let limit = data.meter_limit();
            performance_meter(
                ui,
                "CPU",
                cpu.map(|r| r.tflops / data.ps5_cpu_tflops),
                limit,
                theme::lilac(),
            );
            ui.add_space(6.0);
            performance_meter(
                ui,
                "GPU",
                graphics.map(|r| r.tflops / data.ps5_gpu_tflops),
                limit,
                theme::pink(),
            );
            if gpu.is_some() && !machine.desktop {
                text(ui, "GPU ratings cover desktop models only.", theme::muted());
            }
        });
    }
}

fn section_break(ui: &mut Ui) {
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
}

fn text(ui: &mut Ui, value: &str, color: Color32) {
    ui.add(egui::Label::new(RichText::new(value).font(theme::regular(13.0)).color(color)).wrap());
}

fn performance_meter(ui: &mut Ui, label: &str, ratio: Option<f32>, limit: f32, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .font(theme::strong(13.0))
                .color(theme::white()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let value = ratio.map_or_else(|| "No match".to_owned(), |r| format!("{r:.2}×"));
            ui.label(
                RichText::new(value)
                    .font(theme::strong(13.0))
                    .color(if ratio.is_some() { color } else { theme::muted() }),
            );
        });
    });
    let Some(ratio) = ratio else { return };
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 10.0), egui::Sense::hover());
    let track = rect.shrink2(egui::vec2(0.0, 2.0));
    let x = |r: f32| track.left() + track.width() * (r / limit).clamp(0.0, 1.0);
    ui.painter().rect_filled(track, 3.0, theme::raised());
    let fill = egui::Rect::from_min_max(track.min, egui::pos2(x(ratio), track.bottom()));
    ui.painter().rect_filled(fill, 3.0, color);
    ui.painter()
        .vline(x(1.0), rect.y_range(), egui::Stroke::new(2.0_f32, theme::white()));
    response.on_hover_text(format!(
        "{label} theoretical FP32 throughput relative to PS5. The white marker is 1×."
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meters_share_a_scale_covering_all_reference_models() {
        let data = reference();
        let limit = data.meter_limit();
        for (rows, baseline) in [
            (&data.cpus, data.ps5_cpu_tflops),
            (&data.gpus, data.ps5_gpu_tflops),
        ] {
            assert!(baseline.is_finite() && baseline > 0.0);
            assert!(rows.iter().all(|r| r.tflops / baseline <= limit));
        }
        let cpu = rating(&data.cpus, "AMD Ryzen 7 7800X3D 8-Core Processor", None).unwrap();
        let gpu = rating(&data.gpus, "NVIDIA GeForce RTX 4090", None).unwrap();
        assert_eq!(format!("{:.2}", cpu.tflops / data.ps5_cpu_tflops), "2.78");
        assert_eq!(format!("{:.2}", gpu.tflops / data.ps5_gpu_tflops), "8.02");
    }

    #[test]
    fn reference_uses_exact_model_and_memory_variants() {
        let data = reference();
        assert_eq!(
            rating(&data.gpus, "NVIDIA GeForce RTX 3070", None)
                .unwrap()
                .tflops,
            20.3
        );
        assert_eq!(
            rating(&data.gpus, "NVIDIA GeForce RTX 3070 Ti", None)
                .unwrap()
                .tflops,
            21.7
        );
        assert!(rating(&data.gpus, "NVIDIA GeForce RTX 3070 Laptop GPU", Some(8)).is_none());
        assert!(rating(&data.gpus, "NVIDIA GeForce RTX 3060", Some(8)).is_none());
        assert!(rating(&data.gpus, "NVIDIA GeForce RTX 3060", Some(12)).is_some());
        assert!(rating(&data.gpus, "NVIDIA GeForce RTX 3060", None).is_none());
        assert!(rating(&data.gpus, "AMD Radeon(TM) Graphics", None).is_none());
        assert_eq!(
            rating(&data.cpus, "AMD Ryzen 7 7800X3D 8-Core Processor", None)
                .unwrap()
                .tflops,
            1.25
        );
        assert_eq!(
            rating(&data.cpus, "13th Gen Intel(R) Core(TM) i7-13700K", None)
                .unwrap()
                .tflops,
            1.65
        );
        assert!(rating(&data.cpus, "Intel(R) Core(TM) i7-13700HX", None).is_none());
        for entries in [&data.gpus, &data.cpus] {
            let mut keys = std::collections::BTreeSet::new();
            for row in entries {
                assert!(row.tflops.is_finite() && row.tflops > 0.0);
                for name in &row.models {
                    assert!(keys.insert(model_key(name)), "duplicate {name}");
                }
            }
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "reads the local PC’s Windows hardware inventory"]
    fn inspect_local_hardware() {
        let machine = detect();
        println!("{machine:?}");
        assert!(machine.ram_kib.is_some() || machine.cpu.is_some() || !machine.gpus.is_empty());
    }
}
