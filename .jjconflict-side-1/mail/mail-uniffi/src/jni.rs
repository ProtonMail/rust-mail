use muon::tls::errors::Error;
use muon::tls::objects::{GlobalRef, JClass, JObject};
use muon::tls::{JNIEnv, JavaVM};
use std::sync::Once;

static INIT_ONCE: Once = Once::new();

#[unsafe(export_name = "Java_uniffi_proton_1mail_1uniffi_RustInit_init_1tls")]
pub extern "C" fn init_tls(env: JNIEnv<'_>, cls: JClass<'_>) {
    INIT_ONCE.call_once(|| {
        if let Err(e) = try_init_tls(env, cls) {
            panic!("failed to initialize TLS: {e}");
        }
    });
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
    fn new(mut env: JNIEnv<'_>, cls: JClass<'_>) -> Result<Box<Self>, Error> {
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
