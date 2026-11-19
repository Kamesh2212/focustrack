# Tauri finds plugin classes and commands by reflection.
-keep class com.focustrack.usage.** { *; }

# WorkManager stores its jobs in a Room database it creates by reflection; R8
# would otherwise remove the generated class and the app crashes at start-up.
-keep class * extends androidx.room.RoomDatabase { <init>(); }
-keep class androidx.work.impl.WorkDatabase_Impl { *; }
-keep class * extends androidx.work.ListenableWorker { <init>(android.content.Context, androidx.work.WorkerParameters); }
