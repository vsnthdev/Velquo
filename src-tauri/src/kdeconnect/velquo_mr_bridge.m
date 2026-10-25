// Velquo MediaRemote helper bridge (loaded via /usr/bin/perl DynaLoader so it inherits
// the com.apple.perl5 platform entitlement required by macOS 15.4+ / 26 / 27 mediaremoted).

#import <Foundation/Foundation.h>
#include <dlfcn.h>
#include <objc/runtime.h>
#include <objc/message.h>
#include <dispatch/dispatch.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

typedef bool (*MRMediaRemoteSendCommand_t)(int command, id userInfo);
typedef void (*MRMediaRemoteSetElapsedTime_t)(double elapsedTime);
typedef void (*MRMediaRemoteGetNowPlayingApplicationPID_t)(dispatch_queue_t queue, void (^completion)(int pid));
typedef void (*MRMediaRemoteRegisterForNowPlayingNotifications_t)(dispatch_queue_t queue);

static void *load_mediaremote(void) {
    static void *handle = NULL;
    if (!handle) {
        handle = dlopen("/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote", RTLD_LAZY);
    }
    return handle;
}

static NSString *s_lastArtworkTrackKey = nil;

static void velquo_mr_emit_state(BOOL forceArtwork) {
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
            s_lastArtworkTrackKey = nil;
            printf("null\n");
            fflush(stdout);
            return;
        }

        SEL selNowPlayingInfo = NSSelectorFromString(@"nowPlayingInfo");
        if (![item respondsToSelector:selNowPlayingInfo]) {
            s_lastArtworkTrackKey = nil;
            printf("null\n");
            fflush(stdout);
            return;
        }

        NSDictionary *info = ((id (*)(id, SEL))objc_msgSend)(item, selNowPlayingInfo);
        if (!info || ![info isKindOfClass:[NSDictionary class]]) {
            s_lastArtworkTrackKey = nil;
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
        if (playbackRate > 0.001) {
            isPlaying = YES;
        } else if (isPlaying && playbackRate <= 0.0) {
            playbackRate = 1.0;
        }

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

        NSString *artworkId = info[@"kMRMediaRemoteNowPlayingInfoArtworkIdentifier"];
        if (![artworkId isKindOfClass:[NSString class]]) {
            artworkId = info[@"kMRMediaRemoteNowPlayingInfoContentItemIdentifier"];
        }
        if (![artworkId isKindOfClass:[NSString class]]) {
            artworkId = @"";
        }

        NSString *trackKey = [NSString stringWithFormat:@"%@|%@|%@|%@|%@",
                              bundleId ?: @"", title ?: @"", artist ?: @"", album ?: @"", artworkId ?: @""];
        BOOL isNewTrack = (!s_lastArtworkTrackKey || ![s_lastArtworkTrackKey isEqualToString:trackKey]);

        const char *includeArtEnv = getenv("VELQUO_MR_INCLUDE_ARTWORK");
        BOOL shouldFetchArt = forceArtwork || isNewTrack || (includeArtEnv && strcmp(includeArtEnv, "1") == 0);

        NSString *artworkBase64 = @"";
        if (shouldFetchArt && playerPath && (title.length > 0 || artist.length > 0 || artworkId.length > 0)) {
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
                        dispatch_semaphore_wait(sem, dispatch_time(DISPATCH_TIME_NOW, 900 * NSEC_PER_MSEC));
                        if (fetchedData) {
                            artData = fetchedData;
                        }
                    }
                }
            }
            if ([artData isKindOfClass:[NSData class]] && [artData length] > 0) {
                artworkBase64 = [artData base64EncodedStringWithOptions:0] ?: @"";
                s_lastArtworkTrackKey = [trackKey copy];
            }
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

void velquo_mr_get(void) {
    velquo_mr_emit_state(NO);
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

// Persistent event-driven MediaRemote bridge process:
// 1. Registers for real-time MediaRemote notifications on the main RunLoop.
// 2. Streams JSON lines to stdout whenever Now Playing metadata or playback state changes.
// 3. Reads commands ("CMD <id>", "SEEK <ms>", "GET", "ART") from stdin, and exits immediately
//    when stdin closes (guaranteeing automatic exit whenever Velquo quits or terminates).
void velquo_mr_stream(void) {
    @autoreleasepool {
        void *handle = load_mediaremote();
        if (!handle) {
            return;
        }

        MRMediaRemoteRegisterForNowPlayingNotifications_t regFn =
            (MRMediaRemoteRegisterForNowPlayingNotifications_t)dlsym(
                handle,
                "MRMediaRemoteRegisterForNowPlayingNotifications"
            );
        if (regFn) {
            regFn(dispatch_get_main_queue());
        }

        MRMediaRemoteSendCommand_t sendCmd =
            (MRMediaRemoteSendCommand_t)dlsym(handle, "MRMediaRemoteSendCommand");
        MRMediaRemoteSetElapsedTime_t setElapsed =
            (MRMediaRemoteSetElapsedTime_t)dlsym(handle, "MRMediaRemoteSetElapsedTime");

        __block uint64_t pendingGen = 0;
        void (^scheduleEmit)(BOOL, int64_t) = ^(BOOL forceArt, int64_t delayMs) {
            uint64_t gen = ++pendingGen;
            dispatch_after(
                dispatch_time(DISPATCH_TIME_NOW, delayMs * NSEC_PER_MSEC),
                dispatch_get_main_queue(),
                ^{
                    if (gen == pendingGen || forceArt) {
                        velquo_mr_emit_state(forceArt);
                    }
                }
            );
        };

        // Emit initial snapshot immediately on startup
        dispatch_async(dispatch_get_main_queue(), ^{
            velquo_mr_emit_state(NO);
        });

        [[NSNotificationCenter defaultCenter]
            addObserverForName:nil
                        object:nil
                         queue:[NSOperationQueue mainQueue]
                    usingBlock:^(NSNotification *note) {
                        NSString *name = note.name;
                        if (!name) return;
                        if ([name hasPrefix:@"kMRMediaRemote"] ||
                            [name hasPrefix:@"_kMRMediaRemote"] ||
                            [name containsString:@"NowPlaying"]) {
                            scheduleEmit(NO, 35);
                        }
                    }];

        // Background stdin reader: handles commands and terminates the process on EOF
        dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INTERACTIVE, 0), ^{
            char buf[256];
            while (fgets(buf, sizeof(buf), stdin) != NULL) {
                if (strncmp(buf, "CMD ", 4) == 0) {
                    int cmdId = atoi(buf + 4);
                    if (sendCmd) {
                        sendCmd(cmdId, nil);
                    }
                    dispatch_async(dispatch_get_main_queue(), ^{
                        scheduleEmit(NO, 75);
                    });
                } else if (strncmp(buf, "SEEK ", 5) == 0) {
                    double posSec = atof(buf + 5) / 1000.0;
                    if (posSec < 0.0) posSec = 0.0;
                    if (setElapsed) {
                        setElapsed(posSec);
                    }
                    dispatch_async(dispatch_get_main_queue(), ^{
                        scheduleEmit(NO, 75);
                    });
                } else if (strncmp(buf, "ART", 3) == 0) {
                    dispatch_async(dispatch_get_main_queue(), ^{
                        scheduleEmit(YES, 5);
                    });
                } else if (strncmp(buf, "GET", 3) == 0) {
                    dispatch_async(dispatch_get_main_queue(), ^{
                        scheduleEmit(NO, 5);
                    });
                }
            }
            // stdin closed (parent Velquo process exited) -> terminate immediately
            _exit(0);
        });

        [[NSRunLoop mainRunLoop] run];
    }
}
