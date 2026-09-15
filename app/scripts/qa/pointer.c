#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <limits.h>
#include <sys/stat.h>
#include <unistd.h>
#include <wayland-client.h>
#include "virtual-pointer.h"

static struct zwlr_virtual_pointer_manager_v1 *manager;
static void global(void *data, struct wl_registry *registry, uint32_t id,
                   const char *name, uint32_t version) {
    (void)data; (void)version;
    if (!strcmp(name, "zwlr_virtual_pointer_manager_v1"))
        manager = wl_registry_bind(registry, id, &zwlr_virtual_pointer_manager_v1_interface, 1);
}
static void removed(void *data, struct wl_registry *registry, uint32_t id) {
    (void)data; (void)registry; (void)id;
}
int main(int argc, char **argv) {
    const char *home = getenv("HOME"), *runtime = getenv("XDG_RUNTIME_DIR");
    const char *qa = getenv("OPEN_ISLAND_QA"), *display_name = getenv("WAYLAND_DISPLAY");
    if (argc != 3 || !home || !runtime || !qa || strcmp(qa, "1") ||
        strncmp(runtime, home, strlen(home)) || runtime[strlen(home)] != '/' ||
        !display_name || strchr(display_name, '/') || getenv("WAYLAND_SOCKET")) return 2;
    char canonical_home[PATH_MAX], canonical_runtime[PATH_MAX];
    struct stat metadata;
    if (!realpath(home, canonical_home) || strcmp(home, canonical_home) ||
        !realpath(runtime, canonical_runtime) || strcmp(runtime, canonical_runtime) ||
        stat(runtime, &metadata) || !S_ISDIR(metadata.st_mode) ||
        (metadata.st_mode & 077) || metadata.st_uid != getuid() ||
        stat(home, &metadata) || !S_ISDIR(metadata.st_mode) ||
        (metadata.st_mode & 077) || metadata.st_uid != getuid()) return 2;
    char *end_x, *end_y;
    long x = strtol(argv[1], &end_x, 10), y = strtol(argv[2], &end_y, 10);
    if (!argv[1][0] || !argv[2][0] || *end_x || *end_y || x < 0 || x >= 1280 || y < 0 || y >= 720) return 2;
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) return 2;
    struct wl_registry *registry = wl_display_get_registry(display);
    const struct wl_registry_listener listener = {global, removed};
    wl_registry_add_listener(registry, &listener, NULL);
    if (wl_display_roundtrip(display) < 0 || !manager) return 2;
    struct zwlr_virtual_pointer_v1 *pointer = zwlr_virtual_pointer_manager_v1_create_virtual_pointer(manager, NULL);
    if (wl_display_roundtrip(display) < 0) return 2;
    struct timespec settle = {0, 100000000};
    nanosleep(&settle, NULL);
    zwlr_virtual_pointer_v1_motion_absolute(pointer, 0, (uint32_t)x, (uint32_t)y, 1280, 720);
    zwlr_virtual_pointer_v1_frame(pointer);
    if (wl_display_roundtrip(display) < 0) return 2;
    zwlr_virtual_pointer_v1_button(pointer, 0, 272, WL_POINTER_BUTTON_STATE_PRESSED);
    zwlr_virtual_pointer_v1_frame(pointer);
    zwlr_virtual_pointer_v1_button(pointer, 0, 272, WL_POINTER_BUTTON_STATE_RELEASED);
    zwlr_virtual_pointer_v1_frame(pointer);
    int status = wl_display_roundtrip(display) < 0 ? 2 : 0;
    struct timespec hold = {2, 0};
    nanosleep(&hold, NULL);
    zwlr_virtual_pointer_v1_destroy(pointer);
    zwlr_virtual_pointer_manager_v1_destroy(manager);
    wl_registry_destroy(registry);
    wl_display_disconnect(display);
    return status;
}
