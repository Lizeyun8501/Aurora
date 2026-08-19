#include <jni.h>
#include <string>
#include <cstdlib>

// --- Rust extern "C" declarations ---

extern "C" {
    uintptr_t aurora_native_new(const char* data_dir);
    char* aurora_native_create_note(uintptr_t handle, const char* title);
    uintptr_t aurora_native_list_notes_count(uintptr_t handle);
    int32_t aurora_native_get_note(
        uintptr_t handle, uintptr_t index,
        char** out_id, char** out_title, char** out_updated
    );
    uintptr_t aurora_native_search_count(uintptr_t handle, const char* query);
    int32_t aurora_native_get_search_result(
        uintptr_t handle, uintptr_t index, const char* query,
        char** out_note_id, char** out_title, char** out_snippet, double* out_score
    );
    int32_t aurora_native_delete_note(uintptr_t handle, const char* note_id);
    void aurora_native_destroy(uintptr_t handle);
    void aurora_string_free(char* ptr);
}

// --- JNI helpers ---

static jstring cstr_to_jstring(JNIEnv* env, char* cstr) {
    if (cstr == nullptr) return nullptr;
    jstring result = env->NewStringUTF(cstr);
    aurora_string_free(cstr);
    return result;
}

// --- JNI methods ---

extern "C" JNIEXPORT jlong JNICALL
Java_com_aurora_note_UniffiAppCore_nativeNew(JNIEnv* env, jclass cls, jstring dataDir) {
    const char* dir = env->GetStringUTFChars(dataDir, nullptr);
    uintptr_t handle = aurora_native_new(dir);
    env->ReleaseStringUTFChars(dataDir, dir);
    return (jlong)handle;
}

extern "C" JNIEXPORT jstring JNICALL
Java_com_aurora_note_UniffiAppCore_nativeCreateNote(JNIEnv* env, jclass cls, jlong handle, jstring jtitle) {
    const char* title = env->GetStringUTFChars(jtitle, nullptr);
    char* result = aurora_native_create_note((uintptr_t)handle, title);
    env->ReleaseStringUTFChars(jtitle, title);
    return cstr_to_jstring(env, result);
}

extern "C" JNIEXPORT jint JNICALL
Java_com_aurora_note_UniffiAppCore_nativeListNotesCount(JNIEnv* env, jclass cls, jlong handle) {
    return (jint)aurora_native_list_notes_count((uintptr_t)handle);
}

extern "C" JNIEXPORT jobjectArray JNICALL
Java_com_aurora_note_UniffiAppCore_nativeGetNote(JNIEnv* env, jclass cls, jlong handle, jint index) {
    char* id = nullptr;
    char* title = nullptr;
    char* updated = nullptr;
    int32_t rc = aurora_native_get_note(
        (uintptr_t)handle, (uintptr_t)index, &id, &title, &updated
    );
    if (rc != 0) return nullptr;

    jclass stringClass = env->FindClass("java/lang/String");
    jobjectArray result = env->NewObjectArray(3, stringClass, nullptr);
    env->SetObjectArrayElement(result, 0, cstr_to_jstring(env, id));
    env->SetObjectArrayElement(result, 1, cstr_to_jstring(env, title));
    env->SetObjectArrayElement(result, 2, cstr_to_jstring(env, updated));
    return result;
}

extern "C" JNIEXPORT jint JNICALL
Java_com_aurora_note_UniffiAppCore_nativeSearchCount(JNIEnv* env, jclass cls, jlong handle, jstring jquery) {
    const char* query = env->GetStringUTFChars(jquery, nullptr);
    uintptr_t count = aurora_native_search_count((uintptr_t)handle, query);
    env->ReleaseStringUTFChars(jquery, query);
    return (jint)count;
}

extern "C" JNIEXPORT jobjectArray JNICALL
Java_com_aurora_note_UniffiAppCore_nativeGetSearchResult(JNIEnv* env, jclass cls, jlong handle, jint index, jstring jquery) {
    const char* query = env->GetStringUTFChars(jquery, nullptr);
    char* note_id = nullptr;
    char* title = nullptr;
    char* snippet = nullptr;
    double score = 0.0;
    int32_t rc = aurora_native_get_search_result(
        (uintptr_t)handle, (uintptr_t)index, query,
        &note_id, &title, &snippet, &score
    );
    env->ReleaseStringUTFChars(jquery, query);
    if (rc != 0) return nullptr;

    jclass objClass = env->FindClass("java/lang/Object");
    jobjectArray result = env->NewObjectArray(4, objClass, nullptr);
    env->SetObjectArrayElement(result, 0, cstr_to_jstring(env, note_id));
    env->SetObjectArrayElement(result, 1, cstr_to_jstring(env, title));
    env->SetObjectArrayElement(result, 2, cstr_to_jstring(env, snippet));

    jclass doubleClass = env->FindClass("java/lang/Double");
    jmethodID doubleValueOf = env->GetStaticMethodID(doubleClass, "valueOf", "(D)Ljava/lang/Double;");
    jobject scoreObj = env->CallStaticObjectMethod(doubleClass, doubleValueOf, score);
    env->SetObjectArrayElement(result, 3, scoreObj);
    return result;
}

extern "C" JNIEXPORT jint JNICALL
Java_com_aurora_note_UniffiAppCore_nativeDeleteNote(JNIEnv* env, jclass cls, jlong handle, jstring jnoteId) {
    const char* noteId = env->GetStringUTFChars(jnoteId, nullptr);
    int32_t rc = aurora_native_delete_note((uintptr_t)handle, noteId);
    env->ReleaseStringUTFChars(jnoteId, noteId);
    return rc;
}

extern "C" JNIEXPORT void JNICALL
Java_com_aurora_note_UniffiAppCore_nativeDestroy(JNIEnv* env, jclass cls, jlong handle) {
    aurora_native_destroy((uintptr_t)handle);
}

extern "C" JNIEXPORT void JNICALL
Java_com_aurora_note_UniffiAppCore_stringFree(JNIEnv* env, jclass cls, jlong ptr) {
    aurora_string_free((char*)ptr);
}
