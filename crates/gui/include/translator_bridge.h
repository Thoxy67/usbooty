// Thin C++ shim exposing the bits of QTranslator + QQmlApplicationEngine
// the Rust side needs at runtime. Compiled by the cc crate from build.rs
// — no cxx / cxx-qt bridge in the loop, just a plain extern "C" ABI so the
// link works in every build environment (including makepkg's clean chroot
// where cxx-qt-build's .file() does not pick up plain `#[cxx::bridge]`
// modules).
//
// Pointer ownership: translator_new()/_delete() pair, the Rust side keeps
// the raw pointer alive for the lifetime of the QCoreApplication.

#pragma once

#include <QCoreApplication>
#include <QQmlApplicationEngine>
#include <QString>
#include <QTranslator>

extern "C" {

QTranslator *usbooty_translator_new();
bool usbooty_translator_load(QTranslator *tr, const char *path);
void usbooty_translator_install(QTranslator *tr);
void usbooty_translator_remove(QTranslator *tr);
void usbooty_translator_delete(QTranslator *tr);
void usbooty_engine_retranslate(QQmlApplicationEngine *engine);

} // extern "C"
