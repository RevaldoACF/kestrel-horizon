use serde_json::Value;
use std::env;
use std::fs::{self, OpenOptions};
use std::os::unix::process::CommandExt;
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
            return Ok(fs::canonicalize(&sibling).unwrap_or(sibling));
        }
    }
    let cwd = env::current_dir().map_err(|e| e.to_string())?;
    for rel in [
        "dist/libkestrel.so",
        "target/release/libkestrel.so",
        "target/debug/libkestrel.so",
        "libkestrel.so",
    ] {
        let p = cwd.join(rel);
        if p.exists() {
            return Ok(fs::canonicalize(&p).unwrap_or(p));
        }
    }
    Err("libkestrel.so not found next to kestrel-inject or in dist/".to_string())
}

fn find_launcher_exe() -> Option<PathBuf> {
    let home = home_dir();
    for cand in [
        home.join(".local/share/horizon-offline-launcher/horizon-offline-launcher"),
        home.join(".local/bin/horizon-offline-launcher"),
        home.join(".local/bin/horizon-launcher"),
    ] {
        if cand.exists() {
            return Some(cand);
        }
    }
    if let Ok(path_env) = env::var("PATH") {
        for dir in path_env.split(':') {
            for name in ["horizon-launcher", "horizon-offline-launcher"] {
                let p = Path::new(dir).join(name);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    None
}

struct HorizonPaths {
    bin: PathBuf,
    resources: PathBuf,
    game_dir: PathBuf,
}

fn find_horizon_paths() -> Result<HorizonPaths, String> {
    let home = home_dir();
    let launcher_dir = home.join(".local/share/horizon-launcher");
    let game_dir = home.join(".local/share/horizon");

    let mut candidate_versions: Vec<String> = Vec::new();
    if let Ok(raw) = fs::read_to_string(launcher_dir.join("instances.json"))
        && let Ok(v) = serde_json::from_str::<Value>(&raw)
        && let Some(ver) = v["instances"]["horizon"]["version"].as_str()
    {
        candidate_versions.push(ver.to_string());
    }
    if let Ok(raw) = fs::read_to_string(launcher_dir.join("settings.json"))
        && let Ok(v) = serde_json::from_str::<Value>(&raw)
        && let Some(ver) = v["selected"].as_str()
    {
        candidate_versions.push(ver.to_string());
    }
    if let Ok(entries) = fs::read_dir(launcher_dir.join("versions")) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                candidate_versions.push(name.to_string());
            }
        }
    }

    for ver in candidate_versions {
        let ver_dir = launcher_dir.join("versions").join(&ver);
        let bin = ver_dir.join("horizon-client");
        if !bin.exists() {
            continue;
        }
        // Locate prepared 26.3 resources folder inside the version directory
        let hc_dir = ver_dir.join(".local/horizon-client");
        if let Ok(subs) = fs::read_dir(&hc_dir) {
            for sub in subs.flatten() {
                let res_dir = sub.path();
                if res_dir.join("horizon-resources.json").exists() || res_dir.is_dir() {
                    return Ok(HorizonPaths {
                        bin,
                        resources: res_dir,
                        game_dir,
                    });
                }
            }
        }
        let fallback_res = hc_dir.join("26.3");
        if fallback_res.exists() {
            return Ok(HorizonPaths {
                bin,
                resources: fallback_res,
                game_dir,
            });
        }
    }

    Err("Could not locate prepared Horizon resources under ~/.local/share/horizon-launcher/versions".to_string())
}

fn ensure_agent_control(game_dir: &Path) {
    let extras_path = game_dir.join("horizon-extras.txt");
    if let Ok(text) = fs::read_to_string(&extras_path) {
        if text.contains("agentControl:false") {
            let updated = text.replace("agentControl:false", "agentControl:true");
            let _ = fs::write(&extras_path, updated);
        } else if !text.contains("agentControl:") {
            let mut updated = text;
            if !updated.ends_with('\n') {
                updated.push('\n');
            }
            updated.push_str("agentControl:true\n");
            let _ = fs::write(&extras_path, updated);
        }
    }
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
    let mut hp = find_horizon_paths()?;
    let mut foreground = false;
    let mut extra_args: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--bin" => {
                i += 1;
                if i < args.len() {
                    hp.bin = PathBuf::from(&args[i]);
                }
            }
            "--fg" => foreground = true,
            "--" => {
                extra_args.extend(args[i + 1..].iter().cloned());
                break;
            }
            other => extra_args.push(other.to_string()),
        }
        i += 1;
    }

    let _ = fs::create_dir_all(hp.game_dir.join("logs"));
    ensure_agent_control(&hp.game_dir);

    let existing_preload = env::var("LD_PRELOAD").unwrap_or_default();
    let preload = if existing_preload.is_empty() {
        lib.display().to_string()
    } else {
        format!("{}:{}", lib.display(), existing_preload)
    };

    let mut cmd = Command::new(&hp.bin);
    cmd.arg("--resources")
        .arg(&hp.resources)
        .arg("--control")
        .args(&extra_args)
        .current_dir(&hp.game_dir)
        .env("LD_PRELOAD", preload)
        .env(
            "PIPEWIRE_PROPS",
            "{ application.name = \"Horizon\" media.role = \"Game\" }",
        );

    if let Some(launcher) = find_launcher_exe() {
        cmd.env("HORIZON_LAUNCHER_EXE", launcher);
    }

    if foreground {
        let status = cmd
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| format!("failed to start {}: {e}", hp.bin.display()))?;
        if !status.success() {
            return Err(format!("horizon-client exited with {status}"));
        }
        return Ok(());
    }

    // Spawn cleanly detached so no terminal window is required or left blocking
    let log_path = hp.game_dir.join("logs/kestrel.log");
    let log_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
        .map_err(|e| format!("failed to open {}: {e}", log_path.display()))?;
    let log_err = log_file.try_clone().map_err(|e| e.to_string())?;

    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_err))
        .spawn()
        .map_err(|e| format!("failed to start {}: {e}", hp.bin.display()))?;

    println!(
        "[kestrel] Horizon started (pid {}) with {} injected.",
        child.id(),
        lib.display()
    );
    println!("[kestrel] Press [`] or [~] inside Minecraft to open the Kestrel menu.");
    Ok(())
}

fn cmd_install() -> Result<(), String> {
    let lib = find_libkestrel()?;
    let hp = find_horizon_paths()?;
    ensure_agent_control(&hp.game_dir);

    let launcher = find_launcher_exe().ok_or_else(|| {
        "horizon-launcher / horizon-offline-launcher not found; use `kestrel-inject play` or `kestrel-inject desktop`".to_string()
    })?;

    let env_val = format!("LD_PRELOAD={}", lib.display());
    let status = Command::new(&launcher)
        .args(["instance", "launch", "horizon", "--env", &env_val])
        .status()
        .map_err(|e| format!("failed to run {}: {e}", launcher.display()))?;
    if !status.success() {
        return Err(format!("{} returned non-zero status", launcher.display()));
    }
    install_desktop_entry()?;
    println!("[kestrel] Installed LD_PRELOAD hook into Horizon Launcher instance 'horizon'.");
    println!("[kestrel] You can now click Play in Horizon or launch 'Horizon (Kestrel Client)' from your app menu with zero terminal.");
    Ok(())
}

fn install_desktop_entry() -> Result<(), String> {
    let self_exe = env::current_exe().map_err(|e| e.to_string())?;
    let self_exe = fs::canonicalize(&self_exe).unwrap_or(self_exe);
    let apps_dir = home_dir().join(".local/share/applications");
    fs::create_dir_all(&apps_dir).map_err(|e| e.to_string())?;
    let desktop_path = apps_dir.join("horizon-kestrel.desktop");
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=Horizon (Kestrel Client)\nGenericName=Minecraft Client\nComment=Launch Horizon with Kestrel Survival Client injected\nExec=\"{}\" play\nIcon=horizon\nTerminal=false\nCategories=Game;\nStartupWMClass=horizon\n",
        self_exe.display()
    );
    fs::write(&desktop_path, content).map_err(|e| e.to_string())?;
    Ok(())
}

fn cmd_uninstall() -> Result<(), String> {
    if let Some(launcher) = find_launcher_exe() {
        let _ = Command::new(&launcher)
            .args(["instance", "launch", "horizon", "--env", ""])
            .status();
    }
    let desktop_path = home_dir().join(".local/share/applications/horizon-kestrel.desktop");
    if desktop_path.exists() {
        let _ = fs::remove_file(&desktop_path);
    }
    println!("[kestrel] Removed Kestrel hook and desktop shortcut.");
    Ok(())
}

fn cmd_status() {
    match find_libkestrel() {
        Ok(p) => println!("libkestrel.so : {}", p.display()),
        Err(e) => println!("libkestrel.so : MISSING ({e})"),
    }
    match find_horizon_paths() {
        Ok(hp) => {
            println!("horizon-client: {}", hp.bin.display());
            println!("resources dir : {}", hp.resources.display());
            println!("game dir      : {}", hp.game_dir.display());
        }
        Err(e) => println!("horizon-client: NOT FOUND ({e})"),
    }
    if let Some(launcher) = find_launcher_exe() {
        println!("launcher      : {}", launcher.display());
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
            "[kestrel] Tip: use `kestrel-inject install` or `kestrel-inject play` so Horizon starts directly with Kestrel injected."
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
    println!("  kestrel-inject play [--fg] [--bin PATH] [-- ARGS...]   Launch pure Horizon with Kestrel injected (detached)");
    println!("  kestrel-inject install                                 Hook Kestrel into Horizon Launcher & app menu");
    println!("  kestrel-inject uninstall                               Remove Kestrel hook and desktop shortcut");
    println!("  kestrel-inject desktop                                 Create Horizon (Kestrel Client) desktop launcher");
    println!("  kestrel-inject attach [PID]                            Inject .so into a running horizon-client PID");
    println!("  kestrel-inject status                                  Show binary, library, resources, and socket status");
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let res = match args.first().map(String::as_str) {
        Some("play") => cmd_play(&args[1..]),
        Some("install") => cmd_install(),
        Some("uninstall") => cmd_uninstall(),
        Some("desktop") => install_desktop_entry().map(|()| {
            println!("[kestrel] Created ~/.local/share/applications/horizon-kestrel.desktop");
        }),
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
