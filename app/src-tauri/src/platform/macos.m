#import <AppKit/AppKit.h>
#import <objc/runtime.h>
#include <stdatomic.h>
static atomic_bool layoutInvalidated = false;
int oi_take_layout_invalidated(void) { return atomic_exchange(&layoutInvalidated, false); }

// No extra ivars: the Tauri-owned window keeps its lifetime and delegate.
@interface OIIslandPanel : NSPanel
@end
@implementation OIIslandPanel
- (BOOL)canBecomeKeyWindow { return [objc_getAssociatedObject(self, @selector(canBecomeKeyWindow)) boolValue]; }
- (BOOL)canBecomeMainWindow { return NO; }
@end

void oi_panel_init(void *handle) {
    NSWindow *window = (__bridge NSWindow *)handle;
    object_setClass(window, [OIIslandPanel class]);
    window.styleMask = NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel;
    window.level = NSStatusWindowLevel + 1;
    window.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces | NSWindowCollectionBehaviorFullScreenAuxiliary | NSWindowCollectionBehaviorStationary;
    window.hidesOnDeactivate = NO;
    window.hasShadow = NO;
    window.opaque = NO;
    window.backgroundColor = NSColor.clearColor;
    [(NSPanel *)window setBecomesKeyOnlyIfNeeded:YES];
    id wake = [NSWorkspace.sharedWorkspace.notificationCenter addObserverForName:NSWorkspaceDidWakeNotification object:nil queue:NSOperationQueue.mainQueue usingBlock:^(NSNotification *note) {
        atomic_store(&layoutInvalidated, true);
    }];
    id display = [NSNotificationCenter.defaultCenter addObserverForName:NSApplicationDidChangeScreenParametersNotification object:nil queue:NSOperationQueue.mainQueue usingBlock:^(NSNotification *note) {
        atomic_store(&layoutInvalidated, true);
    }];
    objc_setAssociatedObject(window, @selector(setBecomesKeyOnlyIfNeeded:), @[wake, display], OBJC_ASSOCIATION_RETAIN_NONATOMIC);
}
void oi_panel_keyboard(void *handle, int active) {
    NSWindow *window = (__bridge NSWindow *)handle;
    objc_setAssociatedObject(window, @selector(canBecomeKeyWindow), @(active != 0), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    if (active) [window makeKeyWindow]; else [window resignKeyWindow];
}
void oi_panel_frame(void *handle, double x, double top, double width, double height) {
    NSWindow *window = (__bridge NSWindow *)handle;
    double primaryTop = NSMaxY(NSScreen.screens.firstObject.frame);
    [window setFrame:NSMakeRect(x, primaryTop - top - height, width, height) display:YES];
}
typedef struct { unsigned id; double x, y, width, height, scale, safe_top, notch_width; } OIScreen;
int oi_screens(OIScreen *out, int capacity) {
    NSArray<NSScreen *> *screens = NSScreen.screens;
    double primaryTop = NSMaxY(screens.firstObject.frame);
    int count = MIN((int)screens.count, capacity);
    for (int i = 0; i < count; i++) {
        NSScreen *screen = screens[i];
        NSRect frame = screen.frame;
        double safe = screen.safeAreaInsets.top;
        NSRect left = screen.auxiliaryTopLeftArea;
        NSRect right = screen.auxiliaryTopRightArea;
        double notch = safe > 0 ? MAX(0, NSMinX(right) - NSMaxX(left)) : 0;
        // macOS global coordinates converted once to a top-left logical space.
        double top = safe > 0 ? NSMaxY(frame) : NSMaxY(screen.visibleFrame);
        out[i] = (OIScreen){[screen.deviceDescription[@"NSScreenNumber"] unsignedIntValue], frame.origin.x, primaryTop - top, frame.size.width, frame.size.height, screen.backingScaleFactor, safe, notch};
    }
    return count;
}
void oi_pointer(double *x, double *y, unsigned *pid) {
    NSPoint mouse = NSEvent.mouseLocation;
    *x = mouse.x;
    *y = NSMaxY(NSScreen.screens.firstObject.frame) - mouse.y;
    *pid = NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier;
}
