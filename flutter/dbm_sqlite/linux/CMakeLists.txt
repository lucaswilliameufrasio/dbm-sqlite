cmake_minimum_required(VERSION 3.10)
project(dbm_sqlite LANGUAGES NONE)

if(NOT CMAKE_SYSTEM_PROCESSOR MATCHES "^(x86_64|AMD64|amd64)$")
  message(FATAL_ERROR "dbm_sqlite currently provides a Linux x86_64 library only")
endif()

set(DBM_SQLITE_LIBRARY "${CMAKE_CURRENT_SOURCE_DIR}/lib/x86_64/libdbm_sqlite_flutter.so")
set(DBM_SQLCIPHER_LIBRARY "${CMAKE_CURRENT_SOURCE_DIR}/lib/x86_64/libdbm_sqlite_cipher.so")
if(NOT EXISTS "${DBM_SQLITE_LIBRARY}")
  message(FATAL_ERROR "dbm_sqlite Linux x86_64 prebuilt library is missing")
endif()
if(NOT EXISTS "${DBM_SQLCIPHER_LIBRARY}")
  message(FATAL_ERROR "dbm_sqlite Linux x86_64 SQLCipher library is missing")
endif()

set(dbm_sqlite_bundled_libraries
  "${DBM_SQLITE_LIBRARY}"
  "${DBM_SQLCIPHER_LIBRARY}"
  PARENT_SCOPE)
