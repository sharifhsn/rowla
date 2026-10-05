// Disposable, non-editable windows for live Taskbar hit/preview/churn checks.
#import <AppKit/AppKit.h>
#include <fcntl.h>
#include <unistd.h>
@interface MinimizedFixtureWindow : NSWindow
@property BOOL closeRequiresRestore;
@end
@implementation MinimizedFixtureWindow
- (id)accessibilityCloseButton {
    return self.closeRequiresRestore && self.miniaturized ? nil : super.accessibilityCloseButton;
}
@end
@interface InteractionFixture : NSApplication <NSWindowDelegate>
@property(strong) NSMutableArray<NSWindow *> *fixtures;
@property(strong) NSMutableData *input;
@property NSUInteger created,closed;
- (void)createWindows:(NSUInteger)count;
@end
@implementation InteractionFixture
- (void)createWindows:(NSUInteger)count {
    NSRect display=NSScreen.mainScreen.visibleFrame;
    for(NSUInteger i=0;i<MIN(count,64);i++){
        NSUInteger serial=++self.created;
        NSWindow *window=[[MinimizedFixtureWindow alloc] initWithContentRect:NSMakeRect(display.origin.x+30+(serial%2)*20,display.origin.y+display.size.height-280-(serial%2)*20,360,210) styleMask:NSWindowStyleMaskTitled|NSWindowStyleMaskClosable|NSWindowStyleMaskMiniaturizable backing:NSBackingStoreBuffered defer:NO];
        window.releasedWhenClosed=NO;window.delegate=self;
        window.title=[NSString stringWithFormat:@"Taskbar QA %03lu %@",(unsigned long)serial,serial%2?@"Blue":@"Gold"];
        window.backgroundColor=serial%2?NSColor.systemBlueColor:NSColor.systemYellowColor;
        NSTextField *label=[NSTextField labelWithString:window.title];label.frame=NSMakeRect(20,80,320,50);label.font=[NSFont boldSystemFontOfSize:24];[window.contentView addSubview:label];
        [self.fixtures addObject:window];[window orderBack:nil];
    }
}
- (void)windowWillClose:(NSNotification *)note{[self.fixtures removeObject:note.object];self.closed++;}
- (void)report {
    NSMutableArray *windows=[NSMutableArray array];
    for(NSWindow *w in self.fixtures)[windows addObject:@{@"id":@(w.windowNumber),@"minimized":@(w.miniaturized),@"key":@(w.keyWindow),@"visible":@(w.visible),@"title":w.title}];
    NSDictionary *result=@{@"at":@([NSDate date].timeIntervalSince1970),@"pid":@(getpid()),@"created":@(self.created),@"closed":@(self.closed),@"windows":windows};
    NSData *data=[NSJSONSerialization dataWithJSONObject:result options:0 error:nil];fwrite(data.bytes,1,data.length,stdout);putchar('\n');fflush(stdout);
}
- (void)poll:(NSTimer *)timer {
    char bytes[4096];ssize_t size=read(STDIN_FILENO,bytes,sizeof(bytes));if(size>0)[self.input appendBytes:bytes length:(NSUInteger)size];
    if(self.input.length>16384){[self terminate:nil];return;}
    NSData *newline=[@"\n" dataUsingEncoding:NSUTF8StringEncoding];
    for(;;){NSRange line=[self.input rangeOfData:newline options:0 range:NSMakeRange(0,self.input.length)];if(line.location==NSNotFound)break;
        NSData *data=[self.input subdataWithRange:NSMakeRange(0,line.location)];[self.input replaceBytesInRange:NSMakeRange(0,line.location+1) withBytes:NULL length:0];
        NSDictionary *command=[NSJSONSerialization JSONObjectWithData:data options:0 error:nil];NSString *op=command[@"op"];NSUInteger count=MIN([command[@"count"] unsignedIntegerValue],64);
        if([op isEqual:@"open"]){[self createWindows:count];}
        else if([op isEqual:@"close"]){for(NSUInteger i=0;i<count&&self.fixtures.count;i++){NSWindow *window=self.fixtures.lastObject;[window close];}}
        else if([op isEqual:@"minimize"]){for(MinimizedFixtureWindow *w in self.fixtures.copy){if(!count)break;if(!w.miniaturized){w.closeRequiresRestore=[command[@"close_requires_restore"] boolValue];[w miniaturize:nil];count--;}}}
        else if([op isEqual:@"report"]){[self report];}
        else if([op isEqual:@"quit"]){for(NSWindow *w in self.fixtures.copy)[w close];[self report];[self terminate:nil];}
    }
}
@end
int main(void){@autoreleasepool{
    InteractionFixture *app=[InteractionFixture sharedApplication];[app setActivationPolicy:NSApplicationActivationPolicyRegular];
    app.fixtures=[NSMutableArray array];app.input=[NSMutableData data];fcntl(STDIN_FILENO,F_SETFL,O_NONBLOCK);
    [app createWindows:2];[app report];
    [NSTimer scheduledTimerWithTimeInterval:0.02 target:app selector:@selector(poll:) userInfo:nil repeats:YES];
    [app run];
}return 0;}
