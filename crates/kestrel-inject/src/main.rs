use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn home_dir() -> PathBuf {
    PathBuf::from(env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()))
}

fn find_libkestrel() -> Result<PathBuf, String> {
    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let sibling = dir.join("libkestrel.so");
        if sibling.exists() {
            return Ok(sibling);
        }
    }
    let cwd = env::current_dir().map_err(|e| e.to_string())?;
    for rel in [
        "target/release/libkestrel.so",
        "target/debug/libkestrel.so",
        "libkestrel.so",
    ] {
        let p = cwd.join(rel);
        if p.exists() {
            return Ok(p);
        }
    }
    Err("libkestrel.so not found; build with `cargo build --release` first".to_string())
}

fn find_horizon_client() -> Result<(PathBuf, PathBuf), String> {
    let home = home_dir();
    let launcher_dir = home.join(".local/share/horizon-launcher");
    let game_dir = home.join(".local/share/horizon");

    if let Ok(raw) = fs::read_to_string(launcher_dir.join("instances.json"))
        && let Ok(v) = serde_json::from_str::<Value>(&raw)
        && let Some(ver) = v["instances"]["horizon"]["version"].as_str()
    {
        let bin = launcher_dir.join("versions").join(ver).join("horizon-client");
        if bin.exists() {
            return Ok((bin, game_dir));
        }
    }

    if let Ok(raw) = fs::read_to_string(launcher_dir.join("settings.json"))
        && let Ok(v) = serde_json::from_str::<Value>(&raw)
        && let Some(ver) = v["selected"].as_str()
    {
        let bin = launcher_dir.join("versions").join(ver).join("horizon-client");
        if bin.exists() {
            return Ok((bin, game_dir));
        }
    }

    let versions_dir = launcher_dir.join("versions");
    if let Ok(entries) = fs::read_dir(&versions_dir) {
        for entry in entries.flatten() {
            let bin = entry.path().join("horizon-client");
            if bin.exists() {
                return Ok((bin, game_dir));
            }
        }
    }

    Err("Could not locate horizon-client under ~/.local/share/horizon-launcher/versions".to_string())
}

fn find_running_horizon_pids() -> Vec<i32> {
    let mut pids = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return pids;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Ok(pid) = name.to_string_lossy().parse::<i32>() else {
            continue;
        };
        if let Ok(exe) = fs::read_link(entry.path().join("exe"))
            && exe.file_name().is_some_and(|n| n == "horizon-client")
        {
            pids.push(pid);
        }
    }
    pids
}

fn cmd_play(args: &[String]) -> Result<(), String> {
    let lib = find_libkestrel()?;
    let (mut bin, game_dir) = find_horizon_client()?;
    let mut extra_args: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--bin" => {
                i += 1;
                if i < args.len() {
                    bin = PathBuf::from(&args[i]);
                }
            }
            "--" => {
                extra_args.extend(args[i + 1..].iter().cloned());
                break;
            }
            other => extra_args.push(other.to_string()),
        }
        i += 1;
    }

    let existing_preload = env::var("LD_PRELOAD").unwrap_or_default();
    let preload = if existing_preload.is_empty() {
        lib.display().to_string()
    } else {
        format!("{}:{}", lib.display(), existing_preload)
    };

    println!("[kestrel] Injecting {} into {}", lib.display(), bin.display());
    println!("[kestrel] Game dir: {}", game_dir.display());
    println!("[kestrel] Press [`] or [~] inside Horizon to open the Kestrel Client menu.");

    let status = Command::new(&bin)
        .args(&extra_args)
        .current_dir(&game_dir)
        .env("LD_PRELOAD", preload)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("failed to start {}: {e}", bin.display()))?;

    if !status.success() {
        return Err(format!("horizon-client exited with {status}"));
    }
    Ok(())
}

fn cmd_install() -> Result<(), String> {
    let lib = find_libkestrel()?;
    let env_val = format!("LD_PRELOAD={}", lib.display());
    let status = Command::new("horizon-launcher")
        .args(["instance", "launch", "horizon", "--env", &env_val])
        .status()
        .map_err(|e| format!("failed to run horizon-launcher: {e}"))?;
    if !status.success() {
        return Err("horizon-launcher instance launch returned non-zero status".to_string());
    }
    println!("[kestrel] Installed LD_PRELOAD hook into Horizon Launcher instance 'horizon'.");
    println!("[kestrel] Click Play in Horizon Launcher, then press [`] or [~] in-game.");
    Ok(())
}

fn cmd_uninstall() -> Result<(), String> {
    let status = Command::new("horizon-launcher")
        .args(["instance", "launch", "horizon", "--env", ""])
        .status()
        .map_err(|e| format!("failed to run horizon-launcher: {e}"))?;
    if !status.success() {
        return Err("horizon-launcher instance launch returned non-zero status".to_string());
    }
    println!("[kestrel] Removed LD_PRELOAD hook from Horizon Launcher instance 'horizon'.");
    Ok(())
}

fn cmd_status() {
    match find_libkestrel() {
        Ok(p) => println!("libkestrel.so : {}", p.display()),
        Err(e) => println!("libkestrel.so : MISSING ({e})"),
    }
    match find_horizon_client() {
        Ok((bin, dir)) => {
            println!("horizon-client: {}", bin.display());
            println!("game dir      : {}", dir.display());
        }
        Err(e) => println!("horizon-client: NOT FOUND ({e})"),
    }
    let pids = find_running_horizon_pids();
    if pids.is_empty() {
        println!("running pids  : none");
    } else {
        println!("running pids  : {:?}", pids);
    }
    let uid = unsafe { libc::geteuid() };
    let sock = PathBuf::from(format!("/run/user/{uid}/horizon-control.sock"));
    println!(
        "control sock  : {} ({})",
        sock.display(),
        if sock.exists() { "active" } else { "inactive" }
    );
}

#[cfg(target_arch = "x86_64")]
fn cmd_attach(pid_arg: Option<&String>) -> Result<(), String> {
    let lib = find_libkestrel()?;
    let pid = match pid_arg {
        Some(s) => s.parse::<i32>().map_err(|_| format!("invalid pid: {s}"))?,
        None => {
            let pids = find_running_horizon_pids();
            *pids
                .first()
                .ok_or_else(|| "no running horizon-client process found".to_string())?
        }
    };

    if let Ok(scope) = fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope")
        && scope.trim() != "0"
    {
        eprintln!(
            "[kestrel] Note: kernel.yama.ptrace_scope is {}. Unprivileged attach to an existing PID may be blocked by Yama.",
            scope.trim()
        );
        eprintln!(
            "[kestrel] Tip: for Vulkan/X11 hooks from frame 0, prefer `kestrel-inject play` or `kestrel-inject install`."
        );
    }

    ptrace_inject_so(pid, &lib)
}

#[cfg(target_arch = "x86_64")]
fn ptrace_inject_so(pid: i32, so_path: &Path) -> Result<(), String> {
    use std::ffi::CString;

    let so_cstr = CString::new(so_path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    let path_bytes = so_cstr.as_bytes_with_nul();

    let (local_libc_base, remote_libc_base) = find_libc_bases(pid)?;
    let local_dlopen = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"dlopen".as_ptr()) } as usize;
    if local_dlopen == 0 {
        return Err("failed to resolve local dlopen".to_string());
    }
    let remote_dlopen = remote_libc_base + (local_dlopen - local_libc_base);

    unsafe {
        if libc::ptrace(libc::PTRACE_ATTACH, pid, 0, 0) != 0 {
            return Err(format!(
                "PTRACE_ATTACH pid {pid} failed: {} (try `kestrel-inject play` or `kestrel-inject install`)",
                std::io::Error::last_os_error()
            ));
        }
        let mut status: libc::c_int = 0;
        libc::waitpid(pid, &mut status, 0);

        let mut saved_regs: libc::user_regs_struct = std::mem::zeroed();
        if libc::ptrace(libc::PTRACE_GETREGS, pid, 0, &mut saved_regs) != 0 {
            libc::ptrace(libc::PTRACE_DETACH, pid, 0, 0);
            return Err("PTRACE_GETREGS failed".to_string());
        }

        // Write .so path on stack below red zone and call dlopen(path, RTLD_NOW)
        let mut regs = saved_regs;
        let path_addr = (regs.rsp - 512) & !0xF;
        let ret_addr = path_addr - 8;
        let mut word_buf = vec![0u8; path_bytes.len().div_ceil(8) * 8];
        word_buf[..path_bytes.len()].copy_from_slice(path_bytes);

        for (i, chunk) in word_buf.chunks_exact(8).enumerate() {
            let word = u64::from_le_bytes(chunk.try_into().unwrap());
            libc::ptrace(
                libc::PTRACE_POKEDATA,
                pid,
                (path_addr + (i * 8) as u64) as *mut libc::c_void,
                word as *mut libc::c_void,
            );
        }
        // Return address = 0 to trigger SIGSEGV cleanly back to ptrace tracer
        libc::ptrace(
            libc::PTRACE_POKEDATA,
            pid,
            ret_addr as *mut libc::c_void,
            0usize as *mut libc::c_void,
        );

        regs.rsp = ret_addr;
        regs.rip = remote_dlopen as u64;
        regs.rdi = path_addr;
        regs.rsi = libc::RTLD_NOW as u64;
        regs.rax = 0;

        libc::ptrace(libc::PTRACE_SETREGS, pid, 0, &regs);
        libc::ptrace(libc::PTRACE_CONT, pid, 0, 0);
        libc::waitpid(pid, &mut status, 0);

        let mut after_regs: libc::user_regs_struct = std::mem::zeroed();
        libc::ptrace(libc::PTRACE_GETREGS, pid, 0, &mut after_regs);
        let handle = after_regs.rax;
        libc::ptrace(libc::PTRACE_SETREGS, pid, 0, &saved_regs);
        libc::ptrace(libc::PTRACE_DETACH, pid, 0, 0);

        if handle == 0 {
            return Err("remote dlopen returned NULL".to_string());
        }
        println!("[kestrel] Injected {} into pid {pid} (handle 0x{handle:x})", so_path.display());
    }
    Ok(())
}

fn find_libc_bases(pid: i32) -> Result<(usize, usize), String> {
    let parse_first = |maps_path: &str| -> Option<usize> {
        let txt = fs::read_to_string(maps_path).ok()?;
        for line in txt.lines() {
            if line.contains("libc.so.6") {
                let start = line.split('-').next()?;
                return usize::from_str_radix(start, 16).ok();
            }
        }
        None
    };
    let local = parse_first("/proc/self/maps").ok_or("local libc.so.6 not found in maps")?;
    let remote = parse_first(&format!("/proc/{pid}/maps"))
        .ok_or_else(|| format!("remote libc.so.6 not found in /proc/{pid}/maps"))?;
    Ok((local, remote))
}

fn print_help() {
    println!("kestrel-inject - Horizon Survival Client Injector");
    println!();
    println!("Usage:");
    println!("  kestrel-inject play [--bin PATH] [-- ARGS...]   Launch horizon-client with Kestrel injected");
    println!("  kestrel-inject install                          Hook Kestrel into Horizon Launcher Play button");
    println!("  kestrel-inject uninstall                        Remove Kestrel hook from Horizon Launcher");
    println!("  kestrel-inject attach [PID]                     Inject .so into a running horizon-client PID");
    println!("  kestrel-inject status                           Show binary, library, and socket status");
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let res = match args.first().map(String::as_str) {
        Some("play") => cmd_play(&args[1..]),
        Some("install") => cmd_install(),
        Some("uninstall") => cmd_uninstall(),
        Some("attach") => cmd_attach(args.get(1)),
        Some("status") => {
            cmd_status();
            Ok(())
        }
        _ => {
            print_help();
            Ok(())
        }
    };
    if let Err(e) = res {
        eprintln!("[kestrel] error: {e}");
        std::process::exit(1);
    }
}
