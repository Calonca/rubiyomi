// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

package dev.calonca.rubiyomi;

import android.app.NativeActivity;
import android.content.res.Configuration;
import android.os.Build;
import android.view.WindowInsets;

public class MainActivity extends NativeActivity {
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

    // Android 15 draws apps edge to edge; Rust keeps clear of the bars with these.
    public int[] insets() {
        WindowInsets insets = getWindow().getDecorView().getRootWindowInsets();
        if (insets == null) {
            return new int[] {0, 0, 0, 0};
        }
        if (Build.VERSION.SDK_INT >= 30) {
            android.graphics.Insets i = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
            return new int[] {i.left, i.top, i.right, i.bottom};
        }
        return new int[] {
            insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(),
            insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom()
        };
    }

    public boolean toBackground() {
        return moveTaskToBack(true);
    }
}
