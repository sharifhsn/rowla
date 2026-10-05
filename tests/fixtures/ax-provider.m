// Disposable background AX provider for native timeout and large-set tests.
#import <AppKit/AppKit.h>
#include <unistd.h>
static int delay_ms;
static NSUInteger requests;
static int churn_cycles, churn_step;
static NSUInteger created, closed;
@interface FixtureApplication : NSApplication
@property(strong) NSMutableArray<NSWindow *> *fixtureWindows;
@end
@implementation FixtureApplication
- (NSArray *)accessibilityWindows {
    requests++;
    if (delay_ms) usleep((useconds_t)delay_ms * 1000);
    return self.fixtureWindows;
}
- (void)churn:(NSTimer *)timer {
    for (int i=0;i<8;i++) {
        NSWindow *old=self.fixtureWindows.firstObject;
        if(old){NSAccessibilityPostNotification(old,NSAccessibilityUIElementDestroyedNotification);[old close];[self.fixtureWindows removeObjectAtIndex:0];closed++;}
        NSWindow *window=[[NSWindow alloc] initWithContentRect:NSMakeRect(-10000,-10000,300,200) styleMask:NSWindowStyleMaskTitled|NSWindowStyleMaskClosable|NSWindowStyleMaskMiniaturizable backing:NSBackingStoreBuffered defer:NO];
        window.releasedWhenClosed=NO;
        window.title=@"Disposable Taskbar AX fixture";
        [self.fixtureWindows addObject:window];created++;
        NSAccessibilityPostNotification(self,NSAccessibilityWindowCreatedNotification);
    }
    if(++churn_step>=churn_cycles){[timer invalidate];printf("{\"created\":%lu,\"closed\":%lu,\"cycles\":%d}\n",(unsigned long)created,(unsigned long)closed,churn_step);fflush(stdout);}
}
@end
int main(int argc,const char **argv) {
    @autoreleasepool {
        delay_ms=argc>1?atoi(argv[1]):250;
        int count=argc>2?atoi(argv[2]):1;
        churn_cycles=argc>3?atoi(argv[3]):0;
        FixtureApplication *app=[FixtureApplication sharedApplication];
        [app setActivationPolicy:NSApplicationActivationPolicyAccessory];
        app.fixtureWindows=[NSMutableArray array];
        for(int i=0;i<count;i++){
            NSWindow *window=[[NSWindow alloc] initWithContentRect:NSMakeRect(-10000,-10000,300,200) styleMask:NSWindowStyleMaskTitled|NSWindowStyleMaskClosable|NSWindowStyleMaskMiniaturizable backing:NSBackingStoreBuffered defer:NO];
            window.title=@"Disposable Taskbar AX fixture";
            window.releasedWhenClosed=NO;
            [app.fixtureWindows addObject:window];
        }
        puts("ready");fflush(stdout);
        if(churn_cycles)[NSTimer scheduledTimerWithTimeInterval:0.03 target:app selector:@selector(churn:) userInfo:nil repeats:YES];
        [app run];
    }
    return 0;
}
