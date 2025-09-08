import groovy.json.JsonSlurper
import java.util.Properties
import java.io.FileInputStream
import java.io.IOException


plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("signing")
    id("com.vanniktech.maven.publish") version "0.33.0"
}

val privateProperties = Properties().apply {
    try {
        load(FileInputStream("${rootProject.projectDir}/private.properties"))
    } catch (e: IOException) {
        logger.warn("private.properties file doesn't exist. Full error message: $e")
    }
}

val gitHubDomain = "githubProtonMailDomain".fromVariable()
val mavenUser = "mavenCentralUsername".fromVariable()
val mavenPassword = "mavenCentralPassword".fromVariable()
val mavenSigningKey = "mavenSigningKey".fromVariable()
val mavenSigningKeyPassword = "mavenSigningKeyPassword".fromVariable()

android {
    namespace = "proton.android.mail.commonrust"
    compileSdk = 35

    defaultConfig {
        minSdk = 29
        targetSdk = 34

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        proguardFiles(
            getDefaultProguardFile("proguard-android-optimize.txt"),
            "proguard-rules.pro"
        )

        ndk {
            abiFilters += "armeabi-v7a"
            abiFilters += "arm64-v8a"
            abiFilters += "x86_64"
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }

    kotlinOptions {
        jvmTarget = "1.8"
    }
}

mavenPublishing {
    group = "me.proton.mail.common"
    version = "0.0.1"
    pom {
        scm {
            connection.set(gitHubDomain)
            developerConnection.set(gitHubDomain)
            url.set(gitHubDomain)
        }
    }
}

signing {
    useInMemoryPgpKeys(mavenSigningKey, mavenSigningKeyPassword)
}

dependencies {
    val ANNOTATION = "1.9.1"
    val COROUTINES = "1.10.2"
    val JNA = "5.17.0"

    implementation("androidx.annotation:annotation:${ANNOTATION}")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:${COROUTINES}")
    implementation("net.java.dev.jna:jna:${JNA}@aar")

    // Ship the rustls platform verifier helper
    implementation(files(findRustlsPlatformVerifierClasses()))
}

fun String.fromVariable(): String {
    val value = System.getenv(this) ?: "${privateProperties[this]}"
    if (value.isEmpty()) {
        logger.warn("Variable $this is not set!")
    }
    return value
}

fun findRustlsPlatformVerifierClasses(): File {
    val PACKAGE_NAME = "rustls-platform-verifier-android"
    val PACKAGE_PATH = "maven/rustls/rustls-platform-verifier/*/rustls-platform-verifier-*.aar"

    val depExec = providers.exec { commandLine("cargo", "metadata", "--format-version", "1") }
    val depText = depExec.standardOutput.asText.get()
    val depJson = JsonSlurper().parseText(depText) as Map<String, Any>

    val packages = depJson.get("packages") as List<Map<String, Any>>
    val verifier = packages.find { it.get("name") == PACKAGE_NAME }
    val manifest = file(verifier?.get("manifest_path") as String)

    val aar = fileTree(manifest.parentFile) { include(PACKAGE_PATH) }.singleFile
    val jar = zipTree(aar).matching { include("classes.jar") }.singleFile

    return jar
}
