// Velquo MediaRemote helper bridge (loaded via /usr/bin/perl DynaLoader so it inherits
// the com.apple.perl5 platform entitlement required by macOS 15.4+ / 26 / 27 mediaremoted).

#import <Foundation/Foundation.h>
#include <dlfcn.h>
#include <objc/runtime.h>
#include <objc/message.h>
#include <dispatch/dispatch.h>
#include <stdlib.h>

typedef bool (*MRMediaRemoteSendCommand_t)(int command, id userInfo);
typedef void (*MRMediaRemoteSetElapsedTime_t)(double elapsedTime);
typedef void (*MRMediaRemoteGetNowPlayingApplicationPID_t)(dispatch_queue_t queue, void (^completion)(int pid));

static void *load_mediaremote(void) {
    static void *handle = NULL;
    if (!handle) {
        handle = dlopen("/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote", RTLD_LAZY);
    }
    return handle;
}

void velquo_mr_get(void) {
    @autoreleasepool {
        void *handle = load_mediaremote();
        if (!handle) {
            printf("null\n");
            fflush(stdout);
            return;
        }

        Class reqCls = NSClassFromString(@"MRNowPlayingRequest");
        if (!reqCls) {
            printf("null\n");
            fflush(stdout);
            return;
        }

        SEL selItem = NSSelectorFromString(@"localNowPlayingItem");
        SEL selPath = NSSelectorFromString(@"localNowPlayingPlayerPath");
        SEL selPlaying = NSSelectorFromString(@"localIsPlaying");

        if (![reqCls respondsToSelector:selItem] || ![reqCls respondsToSelector:selPath]) {
            printf("null\n");
            fflush(stdout);
            return;
        }

        id item = ((id (*)(id, SEL))objc_msgSend)((id)reqCls, selItem);
        id playerPath = ((id (*)(id, SEL))objc_msgSend)((id)reqCls, selPath);
        BOOL isPlaying = NO;
        if ([reqCls respondsToSelector:selPlaying]) {
            isPlaying = ((BOOL (*)(id, SEL))objc_msgSend)((id)reqCls, selPlaying);
        }

        if (!item) {
            printf("null\n");
            fflush(stdout);
            return;
        }

        SEL selNowPlayingInfo = NSSelectorFromString(@"nowPlayingInfo");
        if (![item respondsToSelector:selNowPlayingInfo]) {
            printf("null\n");
            fflush(stdout);
            return;
        }

        NSDictionary *info = ((id (*)(id, SEL))objc_msgSend)(item, selNowPlayingInfo);
        if (!info || ![info isKindOfClass:[NSDictionary class]]) {
            printf("null\n");
            fflush(stdout);
            return;
        }

        NSString *title = info[@"kMRMediaRemoteNowPlayingInfoTitle"];
        if (![title isKindOfClass:[NSString class]]) title = @"";

        NSString *artist = info[@"kMRMediaRemoteNowPlayingInfoArtist"];
        if (![artist isKindOfClass:[NSString class]]) artist = @"";

        NSString *album = info[@"kMRMediaRemoteNowPlayingInfoAlbum"];
        if (![album isKindOfClass:[NSString class]]) album = @"";

        NSNumber *durationNum = info[@"kMRMediaRemoteNowPlayingInfoDuration"];
        double durationSec = [durationNum isKindOfClass:[NSNumber class]] ? [durationNum doubleValue] : 0.0;

        NSNumber *elapsedNum = info[@"kMRMediaRemoteNowPlayingInfoElapsedTime"];
        double elapsedSec = [elapsedNum isKindOfClass:[NSNumber class]] ? [elapsedNum doubleValue] : 0.0;

        NSNumber *rateNum = info[@"kMRMediaRemoteNowPlayingInfoPlaybackRate"];
        double playbackRate = [rateNum isKindOfClass:[NSNumber class]] ? [rateNum doubleValue] : (isPlaying ? 1.0 : 0.0);

        NSDate *timestampDate = info[@"kMRMediaRemoteNowPlayingInfoTimestamp"];
        if (![timestampDate isKindOfClass:[NSDate class]]) {
            timestampDate = info[@"kMRMediaRemoteNowPlayingInfoCurrentPlaybackDate"];
        }
        if ([timestampDate isKindOfClass:[NSDate class]] && playbackRate > 0.0) {
            NSTimeInterval diff = [[NSDate date] timeIntervalSinceDate:timestampDate];
            if (diff > 0.0 && diff < 86400.0) {
                elapsedSec += diff * playbackRate;
                if (durationSec > 0.0 && elapsedSec > durationSec) {
                    elapsedSec = durationSec;
                }
            }
        }

        NSString *appName = @"";
        NSString *bundleId = @"";
        if (playerPath) {
            SEL selClient = NSSelectorFromString(@"client");
            if ([playerPath respondsToSelector:selClient]) {
                id client = ((id (*)(id, SEL))objc_msgSend)(playerPath, selClient);
                if (client) {
                    SEL selDisplayName = NSSelectorFromString(@"displayName");
                    if ([client respondsToSelector:selDisplayName]) {
                        id dn = ((id (*)(id, SEL))objc_msgSend)(client, selDisplayName);
                        if ([dn isKindOfClass:[NSString class]]) appName = dn;
                    }
                    SEL selBundleId = NSSelectorFromString(@"bundleIdentifier");
                    if ([client respondsToSelector:selBundleId]) {
                        id bid = ((id (*)(id, SEL))objc_msgSend)(client, selBundleId);
                        if ([bid isKindOfClass:[NSString class]]) bundleId = bid;
                    }
                }
            }
        }

        // Fetch artwork bytes if VELQUO_MR_INCLUDE_ARTWORK=1
        NSString *artworkBase64 = @"";
        const char *includeArtEnv = getenv("VELQUO_MR_INCLUDE_ARTWORK");
        if (includeArtEnv && strcmp(includeArtEnv, "1") == 0 && playerPath) {
            NSData *artData = info[@"kMRMediaRemoteNowPlayingInfoArtworkData"];
            if (![artData isKindOfClass:[NSData class]] || [artData length] == 0) {
                SEL selInitPath = NSSelectorFromString(@"initWithPlayerPath:");
                SEL selReqArt = NSSelectorFromString(@"requestNowPlayingItemArtworkOnQueue:completion:");
                id req = [reqCls alloc];
                if ([req respondsToSelector:selInitPath]) {
                    req = ((id (*)(id, SEL, id))objc_msgSend)(req, selInitPath, playerPath);
                    if (req && [req respondsToSelector:selReqArt]) {
                        dispatch_queue_t q = dispatch_queue_create("dev.vsnth.velquo.mr.art", DISPATCH_QUEUE_SERIAL);
                        dispatch_semaphore_t sem = dispatch_semaphore_create(0);
                        __block NSData *fetchedData = nil;

                        void (^artCompletion)(id, NSError *) = ^(id artwork, NSError *err) {
                            if (artwork && !err) {
                                SEL selImageData = NSSelectorFromString(@"imageData");
                                if ([artwork respondsToSelector:selImageData]) {
                                    id d = ((id (*)(id, SEL))objc_msgSend)(artwork, selImageData);
                                    if ([d isKindOfClass:[NSData class]] && [d length] > 0) {
                                        fetchedData = [d copy];
                                    }
                                }
                            }
                            dispatch_semaphore_signal(sem);
                        };

                        ((void (*)(id, SEL, id, id))objc_msgSend)(req, selReqArt, q, artCompletion);
                        dispatch_semaphore_wait(sem, dispatch_time(DISPATCH_TIME_NOW, 1200 * NSEC_PER_MSEC));
                        if (fetchedData) {
                            artData = fetchedData;
                        }
                    }
                }
            }
            if ([artData isKindOfClass:[NSData class]] && [artData length] > 0) {
                artworkBase64 = [artData base64EncodedStringWithOptions:0] ?: @"";
            }
        }

        NSString *artworkId = info[@"kMRMediaRemoteNowPlayingInfoArtworkIdentifier"];
        if (![artworkId isKindOfClass:[NSString class]]) {
            artworkId = info[@"kMRMediaRemoteNowPlayingInfoContentItemIdentifier"];
        }
        if (![artworkId isKindOfClass:[NSString class]]) {
            artworkId = @"";
        }

        NSDictionary *outDict = @{
            @"title": title ?: @"",
            @"artist": artist ?: @"",
            @"album": album ?: @"",
            @"appName": appName ?: @"",
            @"bundleId": bundleId ?: @"",
            @"isPlaying": isPlaying ? (id)kCFBooleanTrue : (id)kCFBooleanFalse,
            @"durationMs": @((long long)(durationSec * 1000.0)),
            @"positionMs": @((long long)(elapsedSec * 1000.0)),
            @"artworkId": artworkId ?: @"",
            @"artworkBase64": artworkBase64 ?: @""
        };

        NSError *jsonErr = nil;
        NSData *jsonData = [NSJSONSerialization dataWithJSONObject:outDict options:0 error:&jsonErr];
        if (jsonData && !jsonErr) {
            NSString *jsonStr = [[NSString alloc] initWithData:jsonData encoding:NSUTF8StringEncoding];
            printf("%s\n", [jsonStr UTF8String]);
        } else {
            printf("null\n");
        }
        fflush(stdout);
    }
}

void velquo_mr_control(void) {
    @autoreleasepool {
        void *handle = load_mediaremote();
        if (!handle) return;

        const char *cmdEnv = getenv("VELQUO_MR_COMMAND");
        const char *seekMsEnv = getenv("VELQUO_MR_SEEK_MS");

        dispatch_queue_t q = dispatch_queue_create("dev.vsnth.velquo.mr.ctrl", DISPATCH_QUEUE_SERIAL);

        if (cmdEnv && strlen(cmdEnv) > 0) {
            int cmdId = atoi(cmdEnv);
            MRMediaRemoteSendCommand_t sendCmd = (MRMediaRemoteSendCommand_t)dlsym(handle, "MRMediaRemoteSendCommand");
            if (sendCmd) {
                sendCmd(cmdId, nil);
            }
        }

        if (seekMsEnv && strlen(seekMsEnv) > 0) {
            double posSec = atof(seekMsEnv) / 1000.0;
            if (posSec < 0.0) posSec = 0.0;
            MRMediaRemoteSetElapsedTime_t setElapsed = (MRMediaRemoteSetElapsedTime_t)dlsym(handle, "MRMediaRemoteSetElapsedTime");
            if (setElapsed) {
                setElapsed(posSec);
            }
        }

        // Wait for MediaRemote IPC round-trip before process exits (matches mediaremote-adapter waitForCommandCompletion)
        MRMediaRemoteGetNowPlayingApplicationPID_t getPid = (MRMediaRemoteGetNowPlayingApplicationPID_t)dlsym(handle, "MRMediaRemoteGetNowPlayingApplicationPID");
        if (getPid) {
            dispatch_semaphore_t sem = dispatch_semaphore_create(0);
            getPid(q, ^(int pid) {
                dispatch_semaphore_signal(sem);
            });
            dispatch_semaphore_wait(sem, dispatch_time(DISPATCH_TIME_NOW, 500 * NSEC_PER_MSEC));
        }
    }
}
