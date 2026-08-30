# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# MainActivity static methods are called by exact name from Rust via JNI.
# R8 must not rename them in release builds.
-keepclassmembers class dev.eden.cheats_manager.MainActivity {
    public static *;
}


# Preserve line numbers in stack traces for release crash debugging.
-keepattributes SourceFile,LineNumberTable
-renamesourcefileattribute SourceFile