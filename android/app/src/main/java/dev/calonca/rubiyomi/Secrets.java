// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

package dev.calonca.rubiyomi;

import android.content.Context;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.util.Base64;
import android.util.Log;

import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.security.KeyStore;

import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

// The AES key lives in the Android Keystore and never leaves it; the files
// hold only what it encrypted, in a folder Android never backs up.
final class Secrets {
    private static final String ALIAS = "rubiyomi-secrets";
    private static final String CIPHER = "AES/GCM/NoPadding";

    private final File dir;

    Secrets(Context context) {
        dir = new File(context.getNoBackupFilesDir(), "secrets");
    }

    private static SecretKey key() throws Exception {
        KeyStore store = KeyStore.getInstance("AndroidKeyStore");
        store.load(null);
        if (store.containsAlias(ALIAS)) {
            return ((KeyStore.SecretKeyEntry) store.getEntry(ALIAS, null)).getSecretKey();
        }
        KeyGenerator generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore");
        generator.init(new KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build());
        return generator.generateKey();
    }

    private File file(String name) {
        return new File(dir, name.replaceAll("[^a-z0-9-]", "_"));
    }

    String get(String name) {
        File f = file(name);
        if (!f.exists()) {
            return null;
        }
        try {
            String[] parts = new String(Files.readAllBytes(f.toPath()), StandardCharsets.UTF_8).split(":", 2);
            Cipher cipher = Cipher.getInstance(CIPHER);
            cipher.init(Cipher.DECRYPT_MODE, key(), new GCMParameterSpec(128, Base64.decode(parts[0], Base64.NO_WRAP)));
            return new String(cipher.doFinal(Base64.decode(parts[1], Base64.NO_WRAP)), StandardCharsets.UTF_8);
        } catch (Exception e) {
            Log.w("rubiyomi", "reading " + name + ": " + e);
            return null;
        }
    }

    boolean set(String name, String value) {
        File f = file(name);
        if (value.isEmpty()) {
            return !f.exists() || f.delete();
        }
        try {
            Cipher cipher = Cipher.getInstance(CIPHER);
            cipher.init(Cipher.ENCRYPT_MODE, key());
            byte[] sealed = cipher.doFinal(value.getBytes(StandardCharsets.UTF_8));
            String text = Base64.encodeToString(cipher.getIV(), Base64.NO_WRAP) + ":" + Base64.encodeToString(sealed, Base64.NO_WRAP);
            dir.mkdirs();
            File tmp = new File(dir, f.getName() + ".tmp");
            Files.write(tmp.toPath(), text.getBytes(StandardCharsets.UTF_8));
            return tmp.renameTo(f);
        } catch (Exception e) {
            Log.w("rubiyomi", "storing " + name + ": " + e);
            return false;
        }
    }
}
