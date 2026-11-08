// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

package dev.calonca.rubiyomi;

import android.app.Activity;
import android.app.Application;
import android.app.Fragment;
import android.app.FragmentManager;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.graphics.Rect;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.util.DisplayMetrics;
import android.view.DisplayCutout;
import android.view.View;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;

import java.util.concurrent.atomic.AtomicInteger;

// What Rust calls on Android. NSH's activity is final and passes nothing to Rust, so this
// Application is where the context and the current activity come from.
public final class Bridge extends Application implements Application.ActivityLifecycleCallbacks {
    private static final String ASK_TAG = "dev.calonca.rubiyomi.Ask";
    private static final int NO_ANSWER = 0;
    private static final int GRANTED = 1;
    private static final int REFUSED = 2;

    private static volatile Bridge app;
    private static volatile Activity activity;
    private static volatile boolean keepOn;
    private static volatile boolean fullScreen;
    private static final AtomicInteger answer = new AtomicInteger(NO_ANSWER);

    private Secrets secrets;
    private AnkiDroidBridge anki;

    @Override
    public void onCreate() {
        super.onCreate();
        secrets = new Secrets(this);
        anki = new AnkiDroidBridge(this);
        app = this;
        registerActivityLifecycleCallbacks(this);
    }

    public static String secret(String name) {
        return app.secrets.get(name);
    }

    public static boolean setSecret(String name, String value) {
        return app.secrets.set(name, value);
    }

    public static String ankiCall(String method, String argsJson) {
        return app.anki.call(method, argsJson);
    }

    @SuppressWarnings("deprecation")
    public static void requestAnkiPermission() {
        Activity a = activity;
        if (a == null) {
            return;
        }
        a.runOnUiThread(() -> {
            FragmentManager fragments = a.getFragmentManager();
            Ask ask = (Ask) fragments.findFragmentByTag(ASK_TAG);
            if (ask == null) {
                ask = new Ask();
                fragments.beginTransaction().add(ask, ASK_TAG).commitNow();
            }
            ask.requestPermissions(new String[] {AnkiDroidBridge.PERMISSION}, 1);
        });
    }

    // The answer to the last request, once.
    public static int takePermissionAnswer() {
        return answer.getAndSet(NO_ANSWER);
    }

    public static void keepScreenOn(boolean on) {
        keepOn = on;
        Activity a = activity;
        if (a != null) {
            a.runOnUiThread(() -> applyKeepOn(a));
        }
    }

    private static void applyKeepOn(Activity a) {
        if (keepOn) {
            a.getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        } else {
            a.getWindow().clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        }
    }

    public static void fullScreen(boolean on) {
        fullScreen = on;
        Activity a = activity;
        if (a != null) {
            a.runOnUiThread(() -> applyFullScreen(a));
        }
    }

    // The bars stay hidden; a swipe from the edge shows them for a moment over the app.
    @SuppressWarnings("deprecation")
    private static void applyFullScreen(Activity a) {
        Window window = a.getWindow();
        if (Build.VERSION.SDK_INT >= 30) {
            WindowInsetsController controller = window.getInsetsController();
            if (controller == null) {
                return;
            }
            controller.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            if (fullScreen) {
                controller.hide(WindowInsets.Type.systemBars());
            } else {
                controller.show(WindowInsets.Type.systemBars());
            }
        } else {
            int layout = View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION;
            int hidden = View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                | View.SYSTEM_UI_FLAG_FULLSCREEN
                | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION;
            window.getDecorView().setSystemUiVisibility(fullScreen ? layout | hidden : layout);
        }
    }

    // Material You's accent, or -1 before Android 12, which has none.
    public static int accentColor() {
        if (Build.VERSION.SDK_INT < 31) {
            return -1;
        }
        return app.getResources().getColor(android.R.color.system_accent1_500, app.getTheme()) & 0xFFFFFF;
    }

    // left, top, right, bottom of what the camera takes, in density-independent pixels. Not the
    // bars: those come and go over the reader, which would otherwise lay out again each time.
    public static float[] cutout() {
        Activity a = activity;
        WindowInsets insets = a == null ? null : a.getWindow().getDecorView().getRootWindowInsets();
        DisplayCutout cut = insets == null ? null : insets.getDisplayCutout();
        if (cut == null) {
            return new float[4];
        }
        float density = a.getResources().getDisplayMetrics().density;
        return new float[] {
            cut.getSafeInsetLeft() / density,
            cut.getSafeInsetTop() / density,
            cut.getSafeInsetRight() / density,
            cut.getSafeInsetBottom() / density
        };
    }

    // Width and height of the whole window, in density-independent pixels. It holds from the first
    // frame, while what a view is laid out in shrinks and grows as the bars go.
    @SuppressWarnings("deprecation")
    public static float[] windowSize() {
        Activity a = activity;
        if (a == null) {
            return new float[2];
        }
        float density = a.getResources().getDisplayMetrics().density;
        if (Build.VERSION.SDK_INT >= 30) {
            Rect bounds = a.getWindowManager().getCurrentWindowMetrics().getBounds();
            return new float[] {bounds.width() / density, bounds.height() / density};
        }
        DisplayMetrics metrics = new DisplayMetrics();
        a.getWindowManager().getDefaultDisplay().getRealMetrics(metrics);
        return new float[] {metrics.widthPixels / density, metrics.heightPixels / density};
    }

    public static boolean openUrl(String url) {
        Activity a = activity;
        if (a == null) {
            return false;
        }
        try {
            a.startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(url)));
            return true;
        } catch (ActivityNotFoundException e) {
            return false;
        }
    }

    // A fragment, because only the activity or a fragment of it gets the answer.
    @SuppressWarnings("deprecation")
    public static final class Ask extends Fragment {
        @Override
        public void onRequestPermissionsResult(int code, String[] permissions, int[] results) {
            boolean granted = results.length > 0 && results[0] == PackageManager.PERMISSION_GRANTED;
            answer.set(granted ? GRANTED : REFUSED);
        }
    }

    @Override
    public void onActivityCreated(Activity a, Bundle state) {
        activity = a;
        applyKeepOn(a);
        // From the start: changed on entering the reader, it would move the window then.
        WindowManager.LayoutParams params = a.getWindow().getAttributes();
        params.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
        a.getWindow().setAttributes(params);
    }

    @Override
    public void onActivityResumed(Activity a) {
        activity = a;
        applyFullScreen(a);
    }

    @Override
    public void onActivityDestroyed(Activity a) {
        if (activity == a) {
            activity = null;
        }
    }

    @Override
    public void onActivityStarted(Activity a) {}

    @Override
    public void onActivityPaused(Activity a) {}

    @Override
    public void onActivityStopped(Activity a) {}

    @Override
    public void onActivitySaveInstanceState(Activity a, Bundle state) {}
}
