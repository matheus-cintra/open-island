#import <AppKit/AppKit.h>
#import <objc/runtime.h>
#import <CoreGraphics/CoreGraphics.h>
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
typedef struct { unsigned id; double x, y, width, height, scale, safe_top, notch_width, physical_width_mm; } OIScreen;
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
        out[i] = (OIScreen){[screen.deviceDescription[@"NSScreenNumber"] unsignedIntValue], frame.origin.x, primaryTop - top, frame.size.width, frame.size.height, screen.backingScaleFactor, safe, notch, CGDisplayScreenSize([screen.deviceDescription[@"NSScreenNumber"] unsignedIntValue]).width};
    }
    return count;
}
void oi_pointer(double *x, double *y, unsigned *pid) {
    NSPoint mouse = NSEvent.mouseLocation;
    *x = mouse.x;
    *y = NSMaxY(NSScreen.screens.firstObject.frame) - mouse.y;
    *pid = NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier;
}

// Return -1 for a helper/CLI process, 0 for an app without an icon, or PNG length.
int oi_application_icon(unsigned pid, unsigned char *out, int capacity) {
    __block int length = -1;
    void (^readIcon)(void) = ^{
        @autoreleasepool {
            NSRunningApplication *app = [NSRunningApplication runningApplicationWithProcessIdentifier:(pid_t)pid];
            if (!app || app.activationPolicy == NSApplicationActivationPolicyProhibited) return;
            length = 0;
            NSImage *icon = app.icon;
            if (!icon) return;
            NSBitmapImageRep *bitmap = [[NSBitmapImageRep alloc]
                initWithBitmapDataPlanes:NULL pixelsWide:64 pixelsHigh:64 bitsPerSample:8
                samplesPerPixel:4 hasAlpha:YES isPlanar:NO colorSpaceName:NSDeviceRGBColorSpace
                bytesPerRow:0 bitsPerPixel:0];
            if (!bitmap) return;
            [NSGraphicsContext saveGraphicsState];
            NSGraphicsContext.currentContext = [NSGraphicsContext graphicsContextWithBitmapImageRep:bitmap];
            [icon drawInRect:NSMakeRect(0, 0, 64, 64) fromRect:NSZeroRect
                  operation:NSCompositingOperationCopy fraction:1.0 respectFlipped:YES hints:nil];
            [NSGraphicsContext restoreGraphicsState];
            NSData *png = [bitmap representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
            if (!png || png.length > (NSUInteger)capacity) return;
            memcpy(out, png.bytes, png.length);
            length = (int)png.length;
        }
    };
    if (NSThread.isMainThread) readIcon();
    else dispatch_sync(dispatch_get_main_queue(), readIcon);
    return length;
}
