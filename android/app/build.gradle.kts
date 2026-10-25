// SPDX-License-Identifier: MIT

plugins {
    id("com.android.application")
}

android {
    namespace = "dev.calonca.rubiyomi"
    compileSdk = 36

    defaultConfig {
        applicationId = "dev.calonca.rubiyomi"
        minSdk = 26
        targetSdk = 36
        versionCode = 4
        versionName = "0.4.0"
        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("debug")
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
