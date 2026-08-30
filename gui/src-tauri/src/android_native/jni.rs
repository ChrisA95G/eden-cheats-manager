#[cfg(target_os = "android")]
use serde::Deserialize;

// JNI_OnLoad is called by the JVM when it loads libgui_lib.so (triggered by
// `System.loadLibrary("gui_lib")` in Rust.kt, before any Tauri command runs).
// We capture two things here because non-UI Tokio threads have a different
// class loader and cannot use find_class() to find app-level classes.
#[cfg(target_os = "android")]
static JVM_PTR: std::sync::OnceLock<usize> = std::sync::OnceLock::new();

// GlobalRef to MainActivity class — captured while we're on the UI thread
// where the correct class loader is active. Reused on Tokio threads.
#[cfg(target_os = "android")]
static MAIN_CLASS: std::sync::OnceLock<::jni::objects::GlobalRef> = std::sync::OnceLock::new();

/// Verify every static method that Rust calls on MainActivity by name still
/// exists after R8/ProGuard minification. Logs to logcat via eprintln! (the
/// Tauri logger is not yet initialised at JNI_OnLoad time). If any method is
/// missing the release APK will silently mis-behave; this makes it loud.
#[cfg(target_os = "android")]
fn probe_jni_methods(env: &mut ::jni::JNIEnv, cls: &::jni::objects::JClass) {
    const METHODS: &[(&str, &str)] = &[
        ("selectEdenLoadDirectory", "()V"),
        ("getEdenLoadAccessStatus", "()Ljava/lang/String;"),
        ("safListDirectory", "(Ljava/lang/String;)Ljava/lang/String;"),
        (
            "safWriteTextFile",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        ),
        ("safDeleteFile", "(Ljava/lang/String;)Ljava/lang/String;"),
        (
            "safRemoveEmptyDirectory",
            "(Ljava/lang/String;)Ljava/lang/String;",
        ),
        ("launchIntent", "(Ljava/lang/String;)V"),
        ("returnToApp", "()V"),
        ("startScanService", "()V"),
        ("stopScanService", "()V"),
    ];
    let mut missing: Vec<&str> = Vec::new();
    for (name, sig) in METHODS {
        if env.get_static_method_id(cls, name, sig).is_err() {
            let _ = env.exception_clear();
            missing.push(name);
        }
    }
    if missing.is_empty() {
        eprintln!(
            "[ECM] JNI probe OK — all {} MainActivity methods found",
            METHODS.len()
        );
    } else {
        eprintln!(
            "[ECM] JNI PROBE FAILED — {}/{} methods missing (R8 renamed them): {:?}",
            missing.len(),
            METHODS.len(),
            missing
        );
        eprintln!("[ECM] Fix: -keepclassmembers class dev.eden.cheats_manager.MainActivity {{ public static *; }} in proguard-rules.pro");
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn JNI_OnLoad(
    vm: *mut ::jni::sys::JavaVM,
    _reserved: *mut std::ffi::c_void,
) -> ::jni::sys::jint {
    let _ = JVM_PTR.set(vm as usize);
    // get_env() works here because JNI_OnLoad is called on an already-attached
    // Java thread; find_class works because the app class loader is active.
    'capture: {
        let Ok(vm_ref) = (unsafe { ::jni::JavaVM::from_raw(vm) }) else {
            break 'capture;
        };
        let Ok(mut env) = vm_ref.get_env() else {
            break 'capture;
        };
        let Ok(cls) = env.find_class("dev/eden/cheats_manager/MainActivity") else {
            break 'capture;
        };
        probe_jni_methods(&mut env, &cls);
        let Ok(global) = env.new_global_ref(cls) else {
            break 'capture;
        };
        let _ = MAIN_CLASS.set(global);
    }
    ::jni::sys::JNI_VERSION_1_6
}

/// Attach to the JVM, resolve the cached MainActivity class, and run `f`.
/// Keeps lifetimes correct by scoping env/jcls inside the call.
#[cfg(target_os = "android")]
fn with_main_class<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce(&mut ::jni::JNIEnv, &::jni::objects::JClass) -> Result<T, String>,
{
    use ::jni::{objects::JClass, JavaVM};
    let ptr = JVM_PTR
        .get()
        .copied()
        .ok_or_else(|| "JVM not captured (JNI_OnLoad not called)".to_string())?
        as *mut ::jni::sys::JavaVM;
    let vm = unsafe { JavaVM::from_raw(ptr) }.map_err(|e| format!("JVM: {e}"))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("JNI attach: {e}"))?;
    let cls_global = MAIN_CLASS
        .get()
        .ok_or_else(|| "MainActivity class not captured".to_string())?;
    let jcls = unsafe { JClass::from_raw(cls_global.as_raw()) };
    f(&mut env, &jcls)
}

// ── SAF storage bridge ────────────────────────────────────────────────────────

#[cfg(target_os = "android")]
#[derive(Debug, Deserialize)]
pub(super) struct SafEntry {
    pub(super) name: String,
    pub(super) directory: bool,
}

#[cfg(target_os = "android")]
pub(super) fn jni_noarg_string_call(method: &str) -> Result<String, String> {
    use ::jni::objects::JString;
    with_main_class(|env, jcls| {
        let result = env
            .call_static_method(jcls, method, "()Ljava/lang/String;", &[])
            .map_err(|e| format!("JNI {method}: {e}"))?;
        let object = result.l().map_err(|e| format!("JNI object: {e}"))?;
        if object.is_null() {
            return Err(format!("{method} returned null"));
        }
        env.get_string(&JString::from(object))
            .map(|value| value.into())
            .map_err(|e| format!("JNI string: {e}"))
    })
}

#[cfg(target_os = "android")]
fn jni_saf_path_call(method: &str, relative_path: &str) -> Result<String, String> {
    use ::jni::objects::{JObject, JString, JValue};
    with_main_class(|env, jcls| {
        let path = env
            .new_string(relative_path)
            .map_err(|e| format!("JNI string: {e}"))?;
        let result = env
            .call_static_method(
                jcls,
                method,
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&JObject::from(path))],
            )
            .map_err(|e| format!("JNI {method}: {e}"))?;
        let object = result.l().map_err(|e| format!("JNI object: {e}"))?;
        if object.is_null() {
            return Err(format!("{method} returned null"));
        }
        env.get_string(&JString::from(object))
            .map(|value| value.into())
            .map_err(|e| format!("JNI string: {e}"))
    })
}

#[cfg(target_os = "android")]
fn jni_saf_write_call(relative_path: &str, content: &str) -> Result<String, String> {
    use ::jni::objects::{JObject, JString, JValue};
    with_main_class(|env, jcls| {
        let path = env
            .new_string(relative_path)
            .map_err(|e| format!("JNI string: {e}"))?;
        let content = env
            .new_string(content)
            .map_err(|e| format!("JNI string: {e}"))?;
        let result = env
            .call_static_method(
                jcls,
                "safWriteTextFile",
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[
                    JValue::Object(&JObject::from(path)),
                    JValue::Object(&JObject::from(content)),
                ],
            )
            .map_err(|e| format!("JNI safWriteTextFile: {e}"))?;
        let object = result.l().map_err(|e| format!("JNI object: {e}"))?;
        if object.is_null() {
            return Err("safWriteTextFile returned null".into());
        }
        env.get_string(&JString::from(object))
            .map(|value| value.into())
            .map_err(|e| format!("JNI string: {e}"))
    })
}

#[cfg(target_os = "android")]
pub(super) fn parse_saf_response(response: String) -> Result<String, String> {
    if let Some(error) = response.strip_prefix("ERROR:") {
        Err(error.trim().to_string())
    } else {
        Ok(response)
    }
}

#[cfg(target_os = "android")]
pub(super) fn jni_saf_list_directory(relative_path: &str) -> Result<Vec<SafEntry>, String> {
    let response = parse_saf_response(jni_saf_path_call("safListDirectory", relative_path)?)?;
    serde_json::from_str(&response).map_err(|e| format!("Invalid SAF directory response: {e}"))
}

#[cfg(target_os = "android")]
pub(super) fn jni_saf_write_text_file(relative_path: &str, content: &str) -> Result<(), String> {
    let response = parse_saf_response(jni_saf_write_call(relative_path, content)?)?;
    if response == "OK" {
        Ok(())
    } else {
        Err(format!("Unexpected SAF response: {response}"))
    }
}

#[cfg(target_os = "android")]
pub(super) fn jni_saf_delete_file(relative_path: &str) -> Result<(), String> {
    let response = parse_saf_response(jni_saf_path_call("safDeleteFile", relative_path)?)?;
    if response == "OK" {
        Ok(())
    } else {
        Err(format!("Unexpected SAF response: {response}"))
    }
}

#[cfg(target_os = "android")]
pub(super) fn jni_saf_remove_empty_directory(relative_path: &str) -> Result<(), String> {
    let response =
        parse_saf_response(jni_saf_path_call("safRemoveEmptyDirectory", relative_path)?)?;
    if response == "OK" {
        Ok(())
    } else {
        Err(format!("Unexpected SAF response: {response}"))
    }
}

pub(super) fn launch_uri_from_activity(uri: &str) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use ::jni::objects::{JObject, JValue};
        return with_main_class(|env, jcls| {
            let uri_j = env
                .new_string(uri)
                .map_err(|e| format!("JNI string: {e}"))?;
            env.call_static_method(
                jcls,
                "launchIntent",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&JObject::from(uri_j))],
            )
            .map_err(|e| format!("JNI launchIntent: {e}"))?;
            Ok(())
        });
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = uri;
        Err("launch_uri_from_activity is Android-only".to_string())
    }
}

/// Bring our app to the foreground, pushing Eden to background.
pub(super) fn return_to_foreground() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return with_main_class(|env, jcls| {
            env.call_static_method(jcls, "returnToApp", "()V", &[])
                .map_err(|e| format!("JNI returnToApp: {e}"))?;
            Ok(())
        });
    }

    #[cfg(not(target_os = "android"))]
    {
        Ok(())
    }
}

/// Start ScanForegroundService — puts our app into foreground-service state so that
/// Android 12+ background activity launch restrictions don't apply when we call
/// returnToApp() later.
pub(super) fn start_scan_service() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return with_main_class(|env, jcls| {
            env.call_static_method(jcls, "startScanService", "()V", &[])
                .map_err(|e| format!("JNI startScanService: {e}"))?;
            Ok(())
        });
    }

    #[cfg(not(target_os = "android"))]
    {
        Ok(())
    }
}

/// Stop ScanForegroundService once our app is back in the foreground.
pub(super) fn stop_scan_service() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return with_main_class(|env, jcls| {
            env.call_static_method(jcls, "stopScanService", "()V", &[])
                .map_err(|e| format!("JNI stopScanService: {e}"))?;
            Ok(())
        });
    }

    #[cfg(not(target_os = "android"))]
    {
        Ok(())
    }
}

#[cfg(target_os = "android")]
pub(super) fn select_eden_load_directory_from_activity() -> Result<(), String> {
    with_main_class(|env, jcls| {
        env.call_static_method(jcls, "selectEdenLoadDirectory", "()V", &[])
            .map_err(|e| format!("JNI selectEdenLoadDirectory: {e}"))?;
        Ok(())
    })
}
