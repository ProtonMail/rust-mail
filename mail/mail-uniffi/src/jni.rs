use muon::tls::errors::Error;
use muon::tls::objects::{GlobalRef, JClass, JObject};
use muon::tls::{JNIEnv, JavaVM, sys};
use std::os::raw::c_void;
use std::sync::{Once, OnceLock};

static INIT_ONCE: Once = Once::new();
static VM: OnceLock<JavaVM> = OnceLock::new();

// This function is called when Java calls `System.loadLibrary()`. This is guaranteed
// by the uniffi kotlin bindings.
#[unsafe(export_name = "JNI_OnLoad")]
pub extern "C" fn jni_on_load(vm: JavaVM, _: *mut c_void) -> sys::jint {
    // Quick check to validate everything is working.
    let Ok(_env) = vm.get_env() else {
        return sys::JNI_ERR;
    };

    VM.get_or_init(move || vm);

    // From https://developer.android.com/training/articles/perf-jni#native-libraries
    sys::JNI_VERSION_1_6
}

#[unsafe(export_name = "Java_uniffi_proton_1mail_1uniffi_RustInit_init_1tls")]
pub extern "C" fn init_tls(env: JNIEnv<'_>, cls: JClass<'_>) {
    INIT_ONCE.call_once(|| {
        if let Err(e) = try_init_tls(env, cls) {
            panic!("failed to initialize TLS: {e}");
        }
    });
}

pub(crate) fn register_thread_with_vm() {
    VM.get()
        .expect("VM should have handle")
        .attach_current_thread_permanently()
        .expect("VM attach should succeed");
}

fn try_init_tls(env: JNIEnv<'_>, cls: JClass<'_>) -> Result<(), Error> {
    let runtime = Runtime::new(env, cls)?;

    muon::tls::init_external(Box::leak(runtime));

    Ok(())
}

struct Runtime {
    vm: JavaVM,
    context: GlobalRef,
    class_loader: GlobalRef,
}

impl Runtime {
    fn new(env: JNIEnv<'_>, cls: JClass<'_>) -> Result<Box<Self>, Error> {
        let loader: JObject = env
            .call_method(cls, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
            .try_into()?;

        Ok(Box::new(Self {
            vm: env.get_java_vm()?,
            context: env.new_global_ref(JObject::null())?,
            class_loader: env.new_global_ref(loader)?,
        }))
    }
}

impl muon::tls::Runtime for Runtime {
    fn java_vm(&self) -> &JavaVM {
        &self.vm
    }

    fn context(&self) -> &GlobalRef {
        &self.context
    }

    fn class_loader(&self) -> &GlobalRef {
        &self.class_loader
    }
}
