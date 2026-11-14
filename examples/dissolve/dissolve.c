// Draws the points of dissolve.en with raylib, as glowing dots that slowly sway. A point's colour
// shows how far it has drifted from where it started.
//
// Decay moves points in steps of one code point. To keep the picture calm, what is shown follows
// the real positions at a limited speed.
#include <math.h>
#include <raylib.h>
#include "points.h"

static int ready, count;
static long frames;
static float start_xyz[MAX_POINTS][3], shown[MAX_POINTS][3], target[MAX_POINTS][3];

static float follow(float from, float to, float step) {
    float d = to - from;
    if (d > step) d = step;
    if (d < -step) d = -step;
    return from + d;
}

static Color heat(float drift) {
    // Cool blue when in place, through white to orange and red as the point wanders off.
    float t = fminf(drift / 0.4f, 1);
    float r = fminf(0.4f + 1.6f * t, 1), g = 0.7f + 0.3f * sinf(3.1416f * t) - 0.5f * t * t;
    float b = fmaxf(1 - 1.5f * t, 0.1f);
    return (Color){(unsigned char)(255 * r), (unsigned char)(255 * g), (unsigned char)(255 * b), 255};
}

double frame(const char *shape) {
    if (!ready) {
        SetTraceLogLevel(LOG_WARNING);
        SetConfigFlags(FLAG_WINDOW_RESIZABLE | FLAG_VSYNC_HINT | FLAG_MSAA_4X_HINT);
        InitWindow(1280, 720, "ENTROPY");
        if (!IsWindowReady()) return 0;
        SetTargetFPS(60);
        count = read_points(shape, start_xyz);
        for (int i = 0; i < count; i++)
            for (int k = 0; k < 3; k++) shown[i][k] = start_xyz[i][k];
        ready = 1;
    }
    if (WindowShouldClose()) {
        CloseWindow();
        ready = 0;
        return 0;
    }
    read_points(shape, target);

    // Time comes from the frame count, so a recording plays at the right speed.
    float t = frames++ / 60.0f;
    float yaw = 0.45f * sinf(0.25f * t), pitch = 0.15f * sinf(0.17f * t);
    float w = (float)GetScreenWidth(), h = (float)GetScreenHeight();
    float focal = fminf(w, 1.8f * h) * 0.75f;

    BeginDrawing();
    ClearBackground((Color){4, 4, 10, 255});
    BeginBlendMode(BLEND_ADDITIVE);
    for (int i = 0; i < count; i++) {
        for (int k = 0; k < 3; k++) shown[i][k] = follow(shown[i][k], target[i][k], 0.002f);
        float x = shown[i][0], y = shown[i][1], z = shown[i][2];
        float x1 = x * cosf(yaw) + z * sinf(yaw), z1 = z * cosf(yaw) - x * sinf(yaw);
        float y1 = y * cosf(pitch) - z1 * sinf(pitch), z2 = z1 * cosf(pitch) + y * sinf(pitch);
        float depth = 7 + z2;
        if (depth < 0.5f) continue;
        Vector2 p = {w / 2 + focal * x1 / depth, h / 2 - focal * y1 / depth};
        float dx = x - start_xyz[i][0], dy = y - start_xyz[i][1], dz = z - start_xyz[i][2];
        Color c = heat(sqrtf(dx * dx + dy * dy + dz * dz));
        DrawCircleV(p, 5, Fade(c, 0.05f));
        DrawCircleV(p, 1.6f, Fade(c, 0.8f));
    }
    EndBlendMode();
    EndDrawing();
    return 1000000;
}
