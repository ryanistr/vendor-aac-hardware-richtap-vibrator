/*
 * Entry point for the source-built
 * vendor.oplus.hardware.lmvibrator@1.0::ILinearMotorVibrator service.
 *
 * The shipped binary lived at /odm/bin/hw; this one is installed under
 * /vendor/bin/hw at a path that already carries hal_vibrator_default_exec, so
 * it runs in the same domain as the vibrator HAL and keeps that domain's
 * existing hwservice_manager permissions.
 */

#include <android/log.h>
#include <hidl/HidlTransportSupport.h>
#include <hidl/ServiceManagement.h>
#include <hidl/Status.h>

#include "LinearMotorVibratorService.h"

#define LOG_TAG "LmVibratorHAL"
#define LOGI(...) __android_log_print(ANDROID_LOG_INFO, LOG_TAG, __VA_ARGS__)
#define LOGE(...) __android_log_print(ANDROID_LOG_ERROR, LOG_TAG, __VA_ARGS__)

using android::OK;
using android::sp;
using android::status_t;
using android::hardware::configureRpcThreadpool;
using android::hardware::joinRpcThreadpool;
using android::hardware::details::registerAsServiceInternal;
using vendor::oplus::hardware::lmvibrator::V1_0::ILinearMotorVibrator;
using vendor::oplus::hardware::lmvibrator::implementation::LinearMotorVibrator;

int main() {
    // The framework's LinearMotorVibratorManager calls are synchronous and
    // short; one thread is enough and keeps the motor off the main thread.
    configureRpcThreadpool(1, true /* callerWillJoin */);

    // The pure implementation is registered directly: libhidl wraps it in the
    // generated BnHwLinearMotorVibrator binder (its constructor is registered
    // by LinearMotorVibratorAll.cpp), which is what the framework talks to.
    sp<LinearMotorVibrator> impl = new LinearMotorVibrator();
    sp<ILinearMotorVibrator> service = impl;

    status_t status = registerAsServiceInternal(service, "default");
    if (status != OK) {
        LOGE("failed to register vendor.oplus.hardware.lmvibrator@1.0::ILinearMotorVibrator/default (%d)",
             status);
        return 1;
    }

    LOGI("registered vendor.oplus.hardware.lmvibrator@1.0::ILinearMotorVibrator/default");
    joinRpcThreadpool();
    return 0;
}
