// IRichtapVibrator.aidl
package vendor.aac.hardware.richtap.vibrator;

import vendor.aac.hardware.richtap.vibrator.IRichtapCallback;
import android.os.ParcelFileDescriptor;

@VintfStability
interface IRichtapVibrator {
    void init(in @nullable IRichtapCallback callback);
    void setDynamicScale(int scale, in @nullable IRichtapCallback callback);
    void setF0(int f0, in @nullable IRichtapCallback callback);
    void stop(in @nullable IRichtapCallback callback);
    void setAmplitude(int amplitude, in @nullable IRichtapCallback callback);
    void performHeParam(int interval, int amplitude, int freq, in @nullable IRichtapCallback callback);
    void off(in @nullable IRichtapCallback callback);
    void on(int timeoutMs, in @nullable IRichtapCallback callback);
    int perform(int effect, byte strength, in @nullable IRichtapCallback callback);
    void performEnvelope(in int[] envInfo, boolean steepMode, in @nullable IRichtapCallback callback);
    void performRtp(in ParcelFileDescriptor pfd, in @nullable IRichtapCallback callback);
    void performHe(int looper, int interval, int amplitude, int freq, in int[] patternInfo, in @nullable IRichtapCallback callback);
}