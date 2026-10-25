// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

package dev.calonca.rubiyomi;

import android.app.NativeActivity;
import android.content.Intent;
import android.content.res.Configuration;
import android.database.Cursor;
import android.net.Uri;
import android.os.Build;
import android.provider.OpenableColumns;
import android.view.WindowInsets;

import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.concurrent.ConcurrentLinkedQueue;

public class MainActivity extends NativeActivity {
    private static final int PICK_EPUB = 1;
    private static final int ANKI_PERMISSION = 2;

    private final ConcurrentLinkedQueue<String> events = new ConcurrentLinkedQueue<>();
    private final AnkiDroidBridge anki = new AnkiDroidBridge(this);
    private Secrets secrets;

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            fullScreen();
        }
    }

    // The bars stay hidden; a swipe from the edge shows them for a moment over the app.
    @SuppressWarnings("deprecation")
    private void fullScreen() {
        android.view.Window window = getWindow();
        if (Build.VERSION.SDK_INT >= 28) {
            window.getAttributes().layoutInDisplayCutoutMode =
                android.view.WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
        }
        if (Build.VERSION.SDK_INT >= 30) {
            window.setDecorFitsSystemWindows(false);
            android.view.WindowInsetsController c = window.getInsetsController();
            if (c != null) {
                c.hide(WindowInsets.Type.systemBars());
                c.setSystemBarsBehavior(android.view.WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            window.getDecorView().setSystemUiVisibility(
                android.view.View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                    | android.view.View.SYSTEM_UI_FLAG_FULLSCREEN
                    | android.view.View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                    | android.view.View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                    | android.view.View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                    | android.view.View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION);
        }
    }

    public void pickEpub() {
        runOnUiThread(() -> {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType("*/*");
            intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
            // Some file managers report EPUBs as zip or as unknown bytes.
            intent.putExtra(Intent.EXTRA_MIME_TYPES, new String[] {"application/epub+zip", "application/zip", "application/octet-stream"});
            try {
                startActivityForResult(intent, PICK_EPUB);
            } catch (Exception e) {
                events.add("import-failed\tNo file picker on this device");
            }
        });
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != PICK_EPUB || resultCode != RESULT_OK || data == null) {
            return;
        }
        if (data.getClipData() != null) {
            for (int i = 0; i < data.getClipData().getItemCount(); i++) {
                importUri(data.getClipData().getItemAt(i).getUri());
            }
        } else if (data.getData() != null) {
            importUri(data.getData());
        }
    }

    private void importUri(Uri uri) {
        new Thread(() -> {
            File dir = new File(getCacheDir(), "import");
            File out = new File(dir, System.nanoTime() + ".epub");
            try {
                if (!dir.isDirectory() && !dir.mkdirs()) {
                    throw new Exception("can't create " + dir);
                }
                try (InputStream in = getContentResolver().openInputStream(uri); OutputStream o = new FileOutputStream(out)) {
                    if (in == null) {
                        throw new Exception("the file can't be read");
                    }
                    byte[] buffer = new byte[64 * 1024];
                    int n;
                    while ((n = in.read(buffer)) > 0) {
                        o.write(buffer, 0, n);
                    }
                }
                events.add("imported\t" + out.getAbsolutePath() + "\t" + clean(displayName(uri)));
            } catch (Exception e) {
                out.delete();
                events.add("import-failed\t" + clean(String.valueOf(e.getMessage())));
            }
        }).start();
    }

    private String displayName(Uri uri) {
        try (Cursor c = getContentResolver().query(uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (c != null && c.moveToFirst() && !c.isNull(0)) {
                return c.getString(0);
            }
        } catch (Exception ignored) {
        }
        String last = uri.getLastPathSegment();
        return last == null ? "book.epub" : last;
    }

    private static String clean(String s) {
        return s.replace('\t', ' ').replace('\n', ' ');
    }

    public String pollEvent() {
        return events.poll();
    }

    public int accentColor() {
        if (Build.VERSION.SDK_INT >= 31) {
            return getResources().getColor(android.R.color.system_accent1_500, getTheme()) & 0xFFFFFF;
        }
        return -1;
    }

    public boolean darkMode() {
        int mode = getResources().getConfiguration().uiMode & Configuration.UI_MODE_NIGHT_MASK;
        return mode == Configuration.UI_MODE_NIGHT_YES;
    }

    // The app draws under the bars and the cutout; Rust keeps clear of them with these.
    public int[] insets() {
        WindowInsets insets = getWindow().getDecorView().getRootWindowInsets();
        if (insets == null) {
            return new int[] {0, 0, 0, 0};
        }
        if (Build.VERSION.SDK_INT >= 30) {
            android.graphics.Insets i = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
            return new int[] {i.left, i.top, i.right, i.bottom};
        }
        int[] out = {
            insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(),
            insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom()
        };
        if (Build.VERSION.SDK_INT >= 28 && insets.getDisplayCutout() != null) {
            android.view.DisplayCutout cut = insets.getDisplayCutout();
            out[0] = Math.max(out[0], cut.getSafeInsetLeft());
            out[1] = Math.max(out[1], cut.getSafeInsetTop());
            out[2] = Math.max(out[2], cut.getSafeInsetRight());
            out[3] = Math.max(out[3], cut.getSafeInsetBottom());
        }
        return out;
    }

    public boolean openUrl(String url) {
        try {
            Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse(url));
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            startActivity(intent);
            return true;
        } catch (Exception e) {
            return false;
        }
    }

    public boolean toBackground() {
        return moveTaskToBack(true);
    }

    public String ankiCall(String method, String argsJson) {
        return anki.call(method, argsJson);
    }

    public void requestAnkiPermission() {
        runOnUiThread(() -> requestPermissions(new String[] {AnkiDroidBridge.PERMISSION}, ANKI_PERMISSION));
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] results) {
        super.onRequestPermissionsResult(requestCode, permissions, results);
        if (requestCode == ANKI_PERMISSION) {
            boolean granted = results.length > 0 && results[0] == android.content.pm.PackageManager.PERMISSION_GRANTED;
            events.add("permission\t" + (granted ? "granted" : "denied"));
        }
    }

    private synchronized Secrets secrets() {
        if (secrets == null) {
            secrets = new Secrets(this);
        }
        return secrets;
    }

    public String secret(String name) {
        return secrets().get(name);
    }

    public boolean setSecret(String name, String value) {
        return secrets().set(name, value);
    }

    public int keyboardHeight() {
        WindowInsets insets = getWindow().getDecorView().getRootWindowInsets();
        if (insets == null || Build.VERSION.SDK_INT < 30) {
            return 0;
        }
        return insets.getInsets(WindowInsets.Type.ime()).bottom;
    }

    public String clipboardText() {
        android.content.ClipboardManager clipboard = (android.content.ClipboardManager) getSystemService(CLIPBOARD_SERVICE);
        android.content.ClipData clip = clipboard == null ? null : clipboard.getPrimaryClip();
        if (clip == null || clip.getItemCount() == 0) {
            return null;
        }
        CharSequence text = clip.getItemAt(0).coerceToText(this);
        return text == null ? null : text.toString();
    }

    public void keepScreenOn(boolean on) {
        runOnUiThread(() -> {
            if (on) {
                getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            } else {
                getWindow().clearFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            }
        });
    }

    public void copyText(String text) {
        runOnUiThread(() -> {
            android.content.ClipboardManager clipboard = (android.content.ClipboardManager) getSystemService(CLIPBOARD_SERVICE);
            if (clipboard != null) {
                clipboard.setPrimaryClip(android.content.ClipData.newPlainText("Rubiyomi", text));
            }
        });
    }
}
