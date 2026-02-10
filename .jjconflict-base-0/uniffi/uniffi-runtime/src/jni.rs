use jni::JavaVM;
use jni::sys;
use std::ffi;
use std::sync::OnceLock;

static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();

/// This function is called when Java calls `System.loadLibrary()`.
/// This is guaranteed by the uniffi kotlin bindings.
#[unsafe(export_name = "JNI_OnLoad")]
pub extern "C" fn jni_on_load(vm: JavaVM, _: *mut ffi::c_void) -> sys::jint {
    // Quick check to validate everything is working.
    let Ok(_) = vm.get_env() else {
        return sys::JNI_ERR;
    };

    JAVA_VM.get_or_init(move || vm);

    // From https://developer.android.com/training/articles/perf-jni#native-libraries
    sys::JNI_VERSION_1_6
}

pub fn register_thread_with_vm() {
    JAVA_VM
        .get()
        .expect("VM should have handle")
        .attach_current_thread_permanently()
        .expect("VM attach should succeed");
}
