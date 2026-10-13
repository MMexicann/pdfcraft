//! System print spooler supplied by craft-print. PDF composition and permission checks
//! stay in PdfCraft; these adapters retain the application's print-error surface.

pub use craft_print::{
    Duplex, HANDLED_OPTIONS, HAS_PRINTER_PREFERENCES, Job, Printer, PrinterOption, Scaling, lp_args, lpstat_command, parse_lpoptions, parse_lpstat,
    parse_lpstat_e, parse_ppd, parse_windows_printers, ppd_text, printers, printers_parsed,
};

/// The driver's job options for a printer (CUPS: its PPD, with the queue's current defaults);
/// empty when it has none or on other platforms. Reads files and runs `lpoptions`, so call it
/// when the user asks, not every frame.
pub fn printer_options(printer: &str) -> Vec<PrinterOption> {
    #[cfg(all(unix, not(target_arch = "wasm32")))]
    {
        printer_options_via(printer, std::path::Path::new("/etc/cups/ppd"), std::process::Command::new("lpoptions"))
    }
    #[cfg(not(all(unix, not(target_arch = "wasm32"))))]
    {
        craft_print::printer_options(printer)
    }
}

/// Read printer properties with an explicit cancellation signal. Unix helper processes have
/// a five-second deadline; UI callers should run this off the frame thread.
pub fn printer_options_checked(printer: &str, cancel: &std::sync::atomic::AtomicBool) -> Result<Vec<PrinterOption>, crate::PrintError> {
    #[cfg(all(unix, not(target_arch = "wasm32")))]
    return printer_options_via_deadline(
        printer,
        std::path::Path::new("/etc/cups/ppd"),
        std::process::Command::new("lpoptions"),
        std::time::Duration::from_secs(5),
        cancel,
    )
    .map_err(|error| crate::PrintError::Spool(format!("printer properties: {error}")));
    #[cfg(not(all(unix, not(target_arch = "wasm32"))))]
    {
        let _ = cancel;
        Ok(craft_print::printer_options(printer))
    }
}

/// Read a destination's options with the PPD directory and command supplied, so tests need
/// neither a system queue nor a physical printer. CUPS instances share their queue's PPD but
/// have their own defaults, queried with the complete `queue/instance` destination.
#[cfg(all(unix, not(target_arch = "wasm32")))]
pub(crate) fn printer_options_via(printer: &str, ppd_dir: &std::path::Path, lpoptions: std::process::Command) -> Vec<PrinterOption> {
    printer_options_via_deadline(printer, ppd_dir, lpoptions, std::time::Duration::from_secs(5), &std::sync::atomic::AtomicBool::new(false))
        .unwrap_or_default()
}

#[cfg(all(unix, not(target_arch = "wasm32")))]
pub(crate) fn printer_options_via_deadline(
    printer: &str,
    ppd_dir: &std::path::Path,
    mut lpoptions: std::process::Command,
    timeout: std::time::Duration,
    cancel: &std::sync::atomic::AtomicBool,
) -> std::io::Result<Vec<PrinterOption>> {
    let (queue, instance) = printer.split_once('/').map_or((printer, None), |(q, i)| (q, Some(i)));
    // Retain the conservative name checks, applied to each component. Neither component is
    // a path: reject dot components, backslashes and any additional separator before I/O.
    let valid = |name: &str| {
        !name.is_empty()
            && name.len() <= 64
            && !matches!(name, "." | "..")
            && name.bytes().all(|b| b.is_ascii_graphic() && !matches!(b, b'/' | b'\\' | b':' | b'=' | b'"'))
    };
    if !valid(queue) || instance.is_some_and(|name| !valid(name)) {
        return Ok(Vec::new());
    }
    const PPD_CAP: u64 = 16 << 20;
    let ppd = std::fs::File::open(ppd_dir.join(format!("{queue}.ppd"))).ok().and_then(|f| {
        use std::io::Read as _;
        let mut bytes = Vec::new();
        f.take(PPD_CAP).read_to_end(&mut bytes).ok().map(|_| bytes)
    });
    let mut options = ppd.map(|b| parse_ppd(&ppd_text(&b))).unwrap_or_default();
    let current = match bounded_output(lpoptions.args(["-p", printer, "-l"]), timeout, cancel) {
        Ok(output) => output,
        // A PPD remains useful on systems without the optional CUPS command-line tools.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(options),
        Err(error) => return Err(error),
    };
    let current = parse_lpoptions(&String::from_utf8_lossy(&current));
    for (key, value) in current {
        if let Some(o) = options.iter_mut().find(|o| o.key == key)
            && o.choices.iter().any(|(c, _)| *c == value)
        {
            o.default = value;
        }
    }
    Ok(options)
}

#[cfg(all(unix, not(target_arch = "wasm32")))]
fn bounded_output(
    command: &mut std::process::Command,
    timeout: std::time::Duration,
    cancel: &std::sync::atomic::AtomicBool,
) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek};
    use std::sync::atomic::Ordering;
    // A file instead of a pipe prevents full output or inherited pipes from blocking a reader.
    let mut output = tempfile::tempfile()?;
    if cancel.load(Ordering::Relaxed) {
        return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "lookup cancelled"));
    }
    let mut child = command.stdout(output.try_clone()?).stderr(std::process::Stdio::null()).spawn()?;
    const MAX_OUTPUT: u64 = 1024 * 1024;
    let start = std::time::Instant::now();
    loop {
        let refused = if cancel.load(Ordering::Relaxed) {
            Some(std::io::Error::new(std::io::ErrorKind::Interrupted, "lookup cancelled"))
        } else if output.metadata().map(|m| m.len() > MAX_OUTPUT).unwrap_or(false) {
            Some(std::io::Error::other("printer options exceed 1 MiB"))
        } else if start.elapsed() >= timeout {
            Some(std::io::Error::new(std::io::ErrorKind::TimedOut, "printer did not respond within the lookup deadline"))
        } else {
            None
        };
        if let Some(error) = refused {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return Err(std::io::Error::other("printer options command failed")),
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
    }
    output.rewind()?;
    let mut bytes = Vec::new();
    output.take(MAX_OUTPUT + 1).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_OUTPUT as usize {
        return Err(std::io::Error::other("printer options exceed 1 MiB"));
    }
    Ok(bytes)
}

pub fn submit(pdf: &[u8], job: &Job) -> Result<String, crate::PrintError> {
    craft_print::submit(pdf, job).map_err(|e| crate::PrintError::Spool(e.to_string()))
}

pub fn open_printer_preferences(printer: &str) -> Result<(), crate::PrintError> {
    craft_print::open_printer_preferences(printer).map_err(|e| crate::PrintError::Spool(e.to_string()))
}
