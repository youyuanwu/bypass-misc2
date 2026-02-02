# Copy file if it exists, create symlink for .so
# Usage: cmake -DSRC=<source_file> -DDST=<dest_dir> -P copy_if_exists.cmake

if(EXISTS "${SRC}")
    # Resolve symlinks to get the real file
    get_filename_component(REAL_SRC "${SRC}" REALPATH)
    get_filename_component(REAL_FILENAME "${REAL_SRC}" NAME)
    get_filename_component(REQUESTED_FILENAME "${SRC}" NAME)
    
    # Copy the real file
    file(COPY "${REAL_SRC}" DESTINATION "${DST}")
    message(STATUS "Copied ${REAL_FILENAME} to ${DST}")
    
    # Create symlink from requested name to real file if different (e.g., libisal.so.2 -> libisal.so.2.0.31)
    if(NOT "${REQUESTED_FILENAME}" STREQUAL "${REAL_FILENAME}")
        file(CREATE_LINK "${REAL_FILENAME}" "${DST}/${REQUESTED_FILENAME}" SYMBOLIC)
        message(STATUS "Created symlink ${REQUESTED_FILENAME} -> ${REAL_FILENAME}")
    endif()
    
    # Create symlink without version suffix (libisal.so -> libisal.so.2.0.31)
    string(REGEX REPLACE "\\.[0-9]+\\.[0-9]+\\.[0-9]+$" "" SYMLINK_NAME "${REAL_FILENAME}")
    if(NOT "${SYMLINK_NAME}" STREQUAL "${REAL_FILENAME}")
        file(CREATE_LINK "${REAL_FILENAME}" "${DST}/${SYMLINK_NAME}" SYMBOLIC)
        message(STATUS "Created symlink ${SYMLINK_NAME} -> ${REAL_FILENAME}")
    endif()
else()
    message(STATUS "File ${SRC} does not exist, skipping")
endif()
