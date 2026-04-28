use std::collections::HashMap;
use std::mem::{size_of, size_of_val};
use std::sync::{Mutex, OnceLock};

/// 内存使用监控工具
pub struct MemoryMonitor;

fn allocation_cache() -> &'static Mutex<HashMap<String, (usize, usize)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (usize, usize)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

impl MemoryMonitor {
    pub fn bytes_to_mib(bytes: usize) -> f64 {
        bytes as f64 / (1024.0 * 1024.0)
    }

    /// 获取当前内存使用情况（以MB为单位）
    pub fn get_memory_usage() -> Option<f64> {
        #[cfg(target_os = "linux")]
        {
            use std::fs::File;
            use std::io::Read;

            let mut file = File::open("/proc/self/status").ok()?;
            let mut content = String::new();
            file.read_to_string(&mut content).ok()?;

            for line in content.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<u64>() {
                            return Some(kb as f64 / 1024.0);
                        }
                    }
                }
            }
        }
        None
    }

    /// 获取详细的内存使用情况
    pub fn get_detailed_memory_usage() -> Option<HashMap<String, f64>> {
        #[cfg(target_os = "linux")]
        {
            use std::fs::File;
            use std::io::Read;

            let mut file = File::open("/proc/self/status").ok()?;
            let mut content = String::new();
            file.read_to_string(&mut content).ok()?;

            let mut memory_stats = HashMap::new();

            for line in content.lines() {
                if line.starts_with("Vm") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<u64>() {
                            memory_stats.insert(
                                parts[0].trim_end_matches(":").to_string(),
                                kb as f64 / 1024.0,
                            );
                        }
                    }
                }
            }

            Some(memory_stats)
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    /// 计算对象的大小（以字节为单位）
    pub fn size_of<T>(value: &T) -> usize {
        size_of_val(value)
    }

    /// 打印内存使用情况
    pub fn log_memory_usage(context: &str) {
        if let Some(memory) = Self::get_memory_usage() {
            bevy::log::info!(
                target: "daboyi::memory",
                "{}: {:.2} MB",
                context,
                memory
            );
        }
    }

    /// 打印详细内存使用情况
    pub fn log_detailed_memory_usage(context: &str) {
        if let Some(memory_stats) = Self::get_detailed_memory_usage() {
            bevy::log::info!(
                target: "daboyi::memory",
                "{} detailed memory usage:",
                context
            );
            for (key, value) in memory_stats {
                bevy::log::info!(
                    target: "daboyi::memory",
                    "  {}: {:.2} MB",
                    key,
                    value
                );
            }
        }
    }

    /// 打印集合类型的大小
    pub fn log_collection_size<T>(context: &str, items: &[T]) {
        let item_size = if items.is_empty() {
            0
        } else {
            Self::size_of(&items[0])
        };
        let total_size = item_size * items.len();
        bevy::log::info!(
            target: "daboyi::memory",
            "{}: {} items, {} bytes each, total {} bytes",
            context,
            items.len(),
            item_size,
            total_size
        );
    }

    pub fn log_vec_allocation<T>(context: &str, items: &Vec<T>) {
        let elem_size = size_of::<T>();
        let len_bytes = items.len().saturating_mul(elem_size);
        let cap_bytes = items.capacity().saturating_mul(elem_size);
        let mut cache = allocation_cache().lock().unwrap();
        let key = format!("{context}::vec");
        let snapshot = (items.len(), cap_bytes);
        if cache.get(&key) == Some(&snapshot) {
            return;
        }
        cache.insert(key, snapshot);
        bevy::log::info!(
            target: "daboyi::memory",
            "{context}: len={} cap={} item_size={}B live={:.2} MiB reserved={:.2} MiB",
            items.len(),
            items.capacity(),
            elem_size,
            Self::bytes_to_mib(len_bytes),
            Self::bytes_to_mib(cap_bytes),
        );
    }

    pub fn log_estimated_allocation(context: &str, cpu_bytes: usize, gpu_bytes: usize, note: &str) {
        let mut cache = allocation_cache().lock().unwrap();
        let key = context.to_string();
        let snapshot = (cpu_bytes, gpu_bytes);
        if cache.get(&key) == Some(&snapshot) {
            return;
        }
        cache.insert(key, snapshot);
        bevy::log::info!(
            target: "daboyi::memory",
            "{context}: cpu_estimate={:.2} MiB gpu_estimate={:.2} MiB total_estimate={:.2} MiB ({note})",
            Self::bytes_to_mib(cpu_bytes),
            Self::bytes_to_mib(gpu_bytes),
            Self::bytes_to_mib(cpu_bytes.saturating_add(gpu_bytes)),
        );
    }
}
