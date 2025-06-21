use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

/// NUMA topology information for optimal thread and memory placement
#[derive(Debug, Clone)]
pub struct NumaTopology {
    nodes: Vec<NumaNode>,
    cpu_to_node: HashMap<usize, usize>,
    total_cpus: usize,
    online_cpus: Vec<usize>,
}

/// Information about a single NUMA node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumaNode {
    pub id: usize,
    pub cpus: Vec<usize>,
    pub memory_size_mb: u64,
    pub free_memory_mb: u64,
    pub distance_matrix: Vec<u8>, // Distance to other nodes
}

/// CPU information including cache details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub id: usize,
    pub numa_node: usize,
    pub core_id: usize,
    pub socket_id: usize,
    pub l1_cache_kb: u32,
    pub l2_cache_kb: u32,
    pub l3_cache_kb: u32,
    pub frequency_mhz: u32,
}

impl NumaTopology {
    /// Detect NUMA topology from the system
    pub fn detect() -> Result<Self, Box<dyn std::error::Error>> {
        #[cfg(target_os = "linux")]
        {
            Self::detect_linux()
        }
        #[cfg(target_os = "macos")]
        {
            Self::detect_macos()
        }
        #[cfg(target_os = "windows")]
        {
            Self::detect_windows()
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            Self::detect_fallback()
        }
    }
    
    /// Create a simple single-node topology for non-NUMA systems
    pub fn single_node() -> Self {
        let num_cpus = num_cpus::get();
        let cpus: Vec<usize> = (0..num_cpus).collect();
        
        let node = NumaNode {
            id: 0,
            cpus: cpus.clone(),
            memory_size_mb: Self::get_total_memory_mb(),
            free_memory_mb: Self::get_free_memory_mb(),
            distance_matrix: vec![10], // Distance to self
        };
        
        let cpu_to_node = cpus.iter().map(|&cpu| (cpu, 0)).collect();
        
        Self {
            nodes: vec![node],
            cpu_to_node,
            total_cpus: num_cpus,
            online_cpus: cpus,
        }
    }
    
    /// Get the number of NUMA nodes
    pub fn num_nodes(&self) -> usize {
        self.nodes.len()
    }
    
    /// Get information about a specific NUMA node
    pub fn node(&self, node_id: usize) -> Option<&NumaNode> {
        self.nodes.get(node_id)
    }
    
    /// Get all NUMA nodes
    pub fn nodes(&self) -> &[NumaNode] {
        &self.nodes
    }
    
    /// Get the NUMA node for a specific CPU
    pub fn cpu_node(&self, cpu_id: usize) -> Option<usize> {
        self.cpu_to_node.get(&cpu_id).copied()
    }
    
    /// Get all online CPUs
    pub fn online_cpus(&self) -> &[usize] {
        &self.online_cpus
    }
    
    /// Get total number of CPUs
    pub fn total_cpus(&self) -> usize {
        self.total_cpus
    }
    
    /// Find the best NUMA node for allocation based on CPU affinity
    pub fn best_node_for_cpus(&self, cpus: &[usize]) -> Option<usize> {
        let mut node_scores: HashMap<usize, usize> = HashMap::new();
        
        for &cpu in cpus {
            if let Some(node_id) = self.cpu_node(cpu) {
                *node_scores.entry(node_id).or_insert(0) += 1;
            }
        }
        
        // Return the node with the most CPUs from the given set
        node_scores.into_iter().max_by_key(|&(_, score)| score).map(|(node_id, _)| node_id)
    }
    
    /// Get distance between two NUMA nodes
    pub fn node_distance(&self, from_node: usize, to_node: usize) -> Option<u8> {
        self.nodes.get(from_node)?.distance_matrix.get(to_node).copied()
    }
    
    /// Find the closest nodes to a given node
    pub fn closest_nodes(&self, node_id: usize, count: usize) -> Vec<usize> {
        let node = match self.nodes.get(node_id) {
            Some(node) => node,
            None => return Vec::new(),
        };
        
        let mut node_distances: Vec<(usize, u8)> = node.distance_matrix
            .iter()
            .enumerate()
            .map(|(i, &distance)| (i, distance))
            .collect();
        
        // Sort by distance (ascending)
        node_distances.sort_by_key(|&(_, distance)| distance);
        
        node_distances.into_iter()
            .take(count)
            .map(|(node_id, _)| node_id)
            .collect()
    }
    
    /// Check if the system has NUMA topology
    pub fn is_numa_system(&self) -> bool {
        self.nodes.len() > 1
    }
    
    /// Get optimal CPU placement for a given number of threads
    pub fn optimal_cpu_placement(&self, num_threads: usize) -> Vec<usize> {
        let mut placement = Vec::with_capacity(num_threads);
        
        if !self.is_numa_system() {
            // For non-NUMA systems, just use the first N CPUs
            placement.extend(self.online_cpus.iter().take(num_threads).copied());
            return placement;
        }
        
        // For NUMA systems, distribute threads across nodes
        let threads_per_node = num_threads / self.nodes.len();
        let extra_threads = num_threads % self.nodes.len();
        
        for (node_idx, node) in self.nodes.iter().enumerate() {
            let threads_for_this_node = threads_per_node + if node_idx < extra_threads { 1 } else { 0 };
            
            for &cpu in node.cpus.iter().take(threads_for_this_node) {
                if self.online_cpus.contains(&cpu) {
                    placement.push(cpu);
                }
                
                if placement.len() >= num_threads {
                    break;
                }
            }
            
            if placement.len() >= num_threads {
                break;
            }
        }
        
        placement
    }
    
    #[cfg(target_os = "linux")]
    fn detect_linux() -> Result<Self, Box<dyn std::error::Error>> {
        let mut nodes = Vec::new();
        let mut cpu_to_node = HashMap::new();
        
        // Read NUMA nodes from /sys/devices/system/node/
        let node_dirs = fs::read_dir("/sys/devices/system/node/")?;
        
        for entry in node_dirs {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            
            if name_str.starts_with("node") {
                if let Ok(node_id) = name_str[4..].parse::<usize>() {
                    let node_path = entry.path();
                    
                    // Read CPU list for this node
                    let cpulist_path = node_path.join("cpulist");
                    let cpus = if cpulist_path.exists() {
                        let cpulist = fs::read_to_string(cpulist_path)?;
                        Self::parse_cpu_list(&cpulist.trim())?
                    } else {
                        Vec::new()
                    };
                    
                    // Read memory info
                    let meminfo_path = node_path.join("meminfo");
                    let (memory_size_mb, free_memory_mb) = if meminfo_path.exists() {
                        Self::parse_numa_meminfo(&meminfo_path)?
                    } else {
                        (1024, 512) // Default values
                    };
                    
                    // Read distance matrix
                    let distance_path = node_path.join("distance");
                    let distance_matrix = if distance_path.exists() {
                        let distance_str = fs::read_to_string(distance_path)?;
                        Self::parse_distance_matrix(&distance_str)?
                    } else {
                        vec![10] // Default self-distance
                    };
                    
                    // Update CPU to node mapping
                    for &cpu in &cpus {
                        cpu_to_node.insert(cpu, node_id);
                    }
                    
                    nodes.push(NumaNode {
                        id: node_id,
                        cpus,
                        memory_size_mb,
                        free_memory_mb,
                        distance_matrix,
                    });
                }
            }
        }
        
        // Sort nodes by ID
        nodes.sort_by_key(|node| node.id);
        
        // Get online CPUs
        let online_cpus = Self::get_online_cpus()?;
        let total_cpus = online_cpus.len();
        
        Ok(Self {
            nodes,
            cpu_to_node,
            total_cpus,
            online_cpus,
        })
    }
    
    #[cfg(target_os = "macos")]
    fn detect_macos() -> Result<Self, Box<dyn std::error::Error>> {
        // macOS doesn't have traditional NUMA, but we can detect CPU topology
        // For simplicity, we'll create a single-node topology
        Ok(Self::single_node())
    }
    
    #[cfg(target_os = "windows")]
    fn detect_windows() -> Result<Self, Box<dyn std::error::Error>> {
        // Windows NUMA detection would require Windows API calls
        // For now, fall back to single-node topology
        Ok(Self::single_node())
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    fn detect_fallback() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self::single_node())
    }
    
    #[allow(dead_code)]
    fn parse_cpu_list(cpulist: &str) -> Result<Vec<usize>, Box<dyn std::error::Error>> {
        let mut cpus = Vec::new();
        
        for range in cpulist.split(',') {
            let range = range.trim();
            if range.is_empty() {
                continue;
            }
            
            if let Some(dash_pos) = range.find('-') {
                // Range like "0-3"
                let start: usize = range[..dash_pos].parse()?;
                let end: usize = range[dash_pos + 1..].parse()?;
                cpus.extend(start..=end);
            } else {
                // Single CPU like "5"
                cpus.push(range.parse()?);
            }
        }
        
        cpus.sort_unstable();
        cpus.dedup();
        Ok(cpus)
    }
    
    #[allow(dead_code)]
    fn parse_numa_meminfo(path: &std::path::Path) -> Result<(u64, u64), Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let mut memory_size_mb = 0;
        let mut free_memory_mb = 0;
        
        for line in content.lines() {
            if line.starts_with("Node") && line.contains("MemTotal:") {
                if let Some(size_start) = line.find(": ") {
                    let size_part = &line[size_start + 2..];
                    if let Some(kb_pos) = size_part.find(" kB") {
                        let size_kb: u64 = size_part[..kb_pos].trim().parse()?;
                        memory_size_mb = size_kb / 1024;
                    }
                }
            } else if line.starts_with("Node") && line.contains("MemFree:") {
                if let Some(size_start) = line.find(": ") {
                    let size_part = &line[size_start + 2..];
                    if let Some(kb_pos) = size_part.find(" kB") {
                        let size_kb: u64 = size_part[..kb_pos].trim().parse()?;
                        free_memory_mb = size_kb / 1024;
                    }
                }
            }
        }
        
        Ok((memory_size_mb, free_memory_mb))
    }
    
    #[allow(dead_code)]
    fn parse_distance_matrix(distance_str: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let distances: Result<Vec<u8>, _> = distance_str
            .split_whitespace()
            .map(|s| s.parse::<u8>())
            .collect();
        
        distances.map_err(|e| e.into())
    }
    
    #[allow(dead_code)]
    fn get_online_cpus() -> Result<Vec<usize>, Box<dyn std::error::Error>> {
        #[cfg(target_os = "linux")]
        {
            let online_path = "/sys/devices/system/cpu/online";
            if std::path::Path::new(online_path).exists() {
                let online_str = fs::read_to_string(online_path)?;
                Self::parse_cpu_list(online_str.trim())
            } else {
                // Fallback: assume all CPUs from 0 to num_cpus-1 are online
                Ok((0..num_cpus::get()).collect())
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok((0..num_cpus::get()).collect())
        }
    }
    
    fn get_total_memory_mb() -> u64 {
        #[cfg(target_os = "linux")]
        {
            if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
                for line in meminfo.lines() {
                    if line.starts_with("MemTotal:") {
                        if let Some(kb_str) = line.split_whitespace().nth(1) {
                            if let Ok(kb) = kb_str.parse::<u64>() {
                                return kb / 1024;
                            }
                        }
                    }
                }
            }
        }
        
        // Fallback
        4096 // 4GB default
    }
    
    fn get_free_memory_mb() -> u64 {
        #[cfg(target_os = "linux")]
        {
            if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
                for line in meminfo.lines() {
                    if line.starts_with("MemAvailable:") {
                        if let Some(kb_str) = line.split_whitespace().nth(1) {
                            if let Ok(kb) = kb_str.parse::<u64>() {
                                return kb / 1024;
                            }
                        }
                    }
                }
            }
        }
        
        // Fallback
        2048 // 2GB default
    }
}

/// NUMA-aware CPU affinity controller
pub struct CpuAffinity {
    topology: Arc<NumaTopology>,
}

impl CpuAffinity {
    pub fn new(topology: Arc<NumaTopology>) -> Self {
        Self { topology }
    }
    
    /// Pin the current thread to a specific CPU
    pub fn pin_to_cpu(&self, cpu_id: usize) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(target_os = "linux")]
        {
            use std::mem;
            
            // Use libc to set CPU affinity
            let mut cpu_set: libc::cpu_set_t = unsafe { mem::zeroed() };
            unsafe {
                libc::CPU_ZERO(&mut cpu_set);
                libc::CPU_SET(cpu_id, &mut cpu_set);
                
                let result = libc::sched_setaffinity(
                    0, // Current thread
                    mem::size_of::<libc::cpu_set_t>(),
                    &cpu_set,
                );
                
                if result != 0 {
                    return Err(format!("Failed to set CPU affinity: {}", std::io::Error::last_os_error()).into());
                }
            }
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            // CPU affinity not supported on this platform
            eprintln!("CPU affinity not supported on this platform for CPU {}", cpu_id);
        }
        
        Ok(())
    }
    
    /// Pin the current thread to a NUMA node (any CPU in the node)
    pub fn pin_to_node(&self, node_id: usize) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(node) = self.topology.node(node_id) {
            if let Some(&first_cpu) = node.cpus.first() {
                self.pin_to_cpu(first_cpu)
            } else {
                Err("NUMA node has no CPUs".into())
            }
        } else {
            Err(format!("NUMA node {} not found", node_id).into())
        }
    }
    
    /// Get the current CPU the thread is running on
    pub fn current_cpu(&self) -> Option<usize> {
        #[cfg(target_os = "linux")]
        {
            unsafe {
                let cpu = libc::sched_getcpu();
                if cpu >= 0 {
                    Some(cpu as usize)
                } else {
                    None
                }
            }
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }
    
    /// Get the current NUMA node
    pub fn current_node(&self) -> Option<usize> {
        self.current_cpu().and_then(|cpu| self.topology.cpu_node(cpu))
    }
}

// Thread-local NUMA node tracking
thread_local! {
    static THREAD_NUMA_NODE: std::cell::Cell<Option<usize>> = std::cell::Cell::new(None);
}

/// Set the NUMA node for the current thread
pub fn set_thread_numa_node(node_id: usize) {
    THREAD_NUMA_NODE.with(|node| node.set(Some(node_id)));
}

/// Get the NUMA node for the current thread
pub fn get_thread_numa_node() -> Option<usize> {
    THREAD_NUMA_NODE.with(|node| node.get())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topology_detection() {
        let topology = NumaTopology::detect().unwrap();
        
        assert!(topology.total_cpus() > 0);
        assert!(!topology.online_cpus().is_empty());
        assert!(!topology.nodes().is_empty());
        
        println!("Detected topology: {} nodes, {} CPUs", 
                topology.num_nodes(), topology.total_cpus());
        
        for node in topology.nodes() {
            println!("Node {}: {} CPUs, {} MB memory", 
                    node.id, node.cpus.len(), node.memory_size_mb);
        }
    }
    
    #[test]
    fn test_single_node_topology() {
        let topology = NumaTopology::single_node();
        
        assert_eq!(topology.num_nodes(), 1);
        assert_eq!(topology.total_cpus(), num_cpus::get());
        assert!(!topology.is_numa_system());
    }
    
    #[test]
    fn test_cpu_placement() {
        let topology = NumaTopology::detect().unwrap();
        
        let placement = topology.optimal_cpu_placement(4);
        assert!(!placement.is_empty());
        assert!(placement.len() <= 4);
        
        // All CPUs should be online
        for &cpu in &placement {
            assert!(topology.online_cpus().contains(&cpu));
        }
    }
    
    #[test]
    fn test_cpu_list_parsing() {
        assert_eq!(NumaTopology::parse_cpu_list("0").unwrap(), vec![0]);
        assert_eq!(NumaTopology::parse_cpu_list("0,2,4").unwrap(), vec![0, 2, 4]);
        assert_eq!(NumaTopology::parse_cpu_list("0-3").unwrap(), vec![0, 1, 2, 3]);
        assert_eq!(NumaTopology::parse_cpu_list("0-2,5,7-8").unwrap(), vec![0, 1, 2, 5, 7, 8]);
    }
    
    #[test]
    fn test_cpu_affinity() {
        let topology = Arc::new(NumaTopology::detect().unwrap());
        let affinity = CpuAffinity::new(topology.clone());
        
        // Test getting current CPU (may not work on all platforms)
        if let Some(cpu) = affinity.current_cpu() {
            println!("Current CPU: {}", cpu);
            
            if let Some(node) = affinity.current_node() {
                println!("Current NUMA node: {}", node);
            }
        }
        
        // Test pinning to first CPU (may fail without sufficient privileges)
        if let Some(&first_cpu) = topology.online_cpus().first() {
            match affinity.pin_to_cpu(first_cpu) {
                Ok(_) => println!("Successfully pinned to CPU {}", first_cpu),
                Err(e) => println!("Failed to pin to CPU {}: {}", first_cpu, e),
            }
        }
    }
    
    #[test]
    fn test_thread_local_numa_tracking() {
        assert_eq!(get_thread_numa_node(), None);
        
        set_thread_numa_node(1);
        assert_eq!(get_thread_numa_node(), Some(1));
        
        // Test in another thread
        let handle = std::thread::spawn(|| {
            assert_eq!(get_thread_numa_node(), None);
            set_thread_numa_node(2);
            get_thread_numa_node()
        });
        
        assert_eq!(handle.join().unwrap(), Some(2));
        assert_eq!(get_thread_numa_node(), Some(1)); // Original thread unchanged
    }
}