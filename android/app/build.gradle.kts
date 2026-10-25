// SPDX-License-Identifier: MIT

import java.util.Properties

plugins {
    id("com.android.application")
}

// The release key never enters the repository: its place and passwords come from
// the environment, or from android/keystore.properties (ignored by git).
val keystore = Properties().apply {
    val file = rootProject.file("keystore.properties")
    if (file.exists()) file.inputStream().use { load(it) }
}
fun signing(name: String, env: String): String? = System.getenv(env) ?: keystore.getProperty(name)
val releaseStore = signing("storeFile", "RUBIYOMI_KEYSTORE")

android {
    namespace = "dev.calonca.rubiyomi"
    compileSdk = 36

    defaultConfig {
        applicationId = "dev.calonca.rubiyomi"
        minSdk = 26
        targetSdk = 36
        versionCode = 5
        versionName = "0.5.0"
        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    signingConfigs {
        if (releaseStore != null) {
            create("release") {
                storeFile = file(releaseStore)
                storePassword = signing("storePassword", "RUBIYOMI_KEYSTORE_PASSWORD")
                keyAlias = signing("keyAlias", "RUBIYOMI_KEY_ALIAS")
                keyPassword = signing("keyPassword", "RUBIYOMI_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = if (releaseStore != null) {
                signingConfigs.getByName("release")
            } else {
                logger.warn("No release key set up: the release build is signed with the debug key and can't go to the Play Store.")
                signingConfigs.getByName("debug")
            }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // The dictionaries are xz already: deflating them again only costs time.
    androidResources {
        noCompress += "xz"
    }

    packaging {
        jniLibs {
            // NativeActivity loads the library from the APK's lib directory.
            useLegacyPackaging = true
        }
    }
}
