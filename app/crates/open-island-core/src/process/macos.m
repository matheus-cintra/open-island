#import <AppKit/AppKit.h>
#include <libproc.h>
#include <sys/sysctl.h>
#include <unistd.h>

int oi_pids(int *out, int bytes) { return proc_listallpids(out, bytes); }
int oi_process(int pid, unsigned *parent, char *name, int capacity) {
    struct proc_bsdinfo info = {0};
    if (proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, sizeof(info)) != sizeof(info)) return 0;
    *parent = info.pbi_ppid;
    char path[PROC_PIDPATHINFO_MAXSIZE] = {0};
    if (proc_pidpath(pid, path, sizeof(path)) > 0) {
        const char *base = strrchr(path, '/');
        strlcpy(name, base ? base + 1 : path, capacity);
    } else strlcpy(name, info.pbi_comm, capacity);
    return 1;
}
int oi_cwd(int pid, char *out, int capacity) {
    struct proc_vnodepathinfo info = {0};
    if (proc_pidinfo(pid, PROC_PIDVNODEPATHINFO, 0, &info, sizeof(info)) != sizeof(info)) return 0;
    strlcpy(out, info.pvi_cdir.vip_path, capacity);
    return 1;
}
int oi_procargs(int pid, char *out, size_t *length) {
    int mib[] = {CTL_KERN, KERN_PROCARGS2, pid};
    return sysctl(mib, 3, out, length, NULL, 0) == 0;
}
int oi_activate(int pid) {
    @autoreleasepool {
        NSRunningApplication *app = [NSRunningApplication runningApplicationWithProcessIdentifier:pid];
        if (!app || app.activationPolicy == NSApplicationActivationPolicyProhibited) return -1;
        return [app activateWithOptions:NSApplicationActivateIgnoringOtherApps] ? 1 : 0;
    }
}
