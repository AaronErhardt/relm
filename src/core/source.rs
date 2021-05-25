/*
 * Copyright (c) 2018 Boucher, Antoni <bouanto@zoho.com>
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy of
 * this software and associated documentation files (the "Software"), to deal in
 * the Software without restriction, including without limitation the rights to
 * use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
 * the Software, and to permit persons to whom the Software is furnished to do so,
 * subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
 * FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
 * COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
 * IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
 * CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */

use std::mem;
use std::os::raw::c_int;
use std::ptr;

use glib::translate::{from_glib_full, ToGlibPtr};
use glib::Source;
use glib_sys::{g_source_new, GSource, GSourceFunc, GSourceFuncs};

/// Trait defining an interface to SourceFuncs of a GSource.
/// These functions are called at different stages of the
/// main event loop of glib.
pub trait SourceFuncs {
    fn check(&self) -> bool {
        false
    }

    fn dispatch(&self) -> bool;
    fn prepare(&self) -> (bool, Option<u32>);
}

struct SourceData<T> {
    _source: GSource,
    funcs: Box<GSourceFuncs>,
    data: T,
}

/// Create a new source that interacts with the main event loop of glib.
/// More infotmation about glib and the main event loop can be found
/// [here](https://developer.gnome.org/glib/stable/glib-The-Main-Event-Loop.html).
pub fn new_source<T: SourceFuncs>(data: T) -> Source {
    unsafe {
        let mut funcs = Box::new(GSourceFuncs {
            prepare: Some(prepare::<T>),
            check: Some(check::<T>),
            dispatch: Some(dispatch::<T>),
            finalize: Some(finalize::<T>),
            closure_marshal: mem::zeroed(),
            closure_callback: mem::zeroed(),
        });
        let source: *mut GSource =
            g_source_new(&mut *funcs, mem::size_of::<SourceData<T>>() as u32);
        ptr::write(&mut (*(source as *mut SourceData<T>)).data, data);
        ptr::write(&mut (*(source as *mut SourceData<T>)).funcs, funcs);
        from_glib_full(source)
    }
}

pub fn source_get<T: SourceFuncs>(source: &Source) -> &T {
    unsafe { &(*(source.to_glib_none().0 as *const SourceData<T>)).data }
}

/// Call check() on data in GSource
unsafe extern "C" fn check<T: SourceFuncs>(source: *mut GSource) -> c_int {
    let object = source as *mut SourceData<T>;
    bool_to_int((*object).data.check())
}

/// Call dispatch() on data in GSource
unsafe extern "C" fn dispatch<T: SourceFuncs>(
    source: *mut GSource,
    _callback: GSourceFunc,
    _user_data: *mut libc::c_void,
) -> c_int {
    let object = source as *mut SourceData<T>;
    bool_to_int((*object).data.dispatch())
}

/// Call finalize on data in GSource
unsafe extern "C" fn finalize<T: SourceFuncs>(source: *mut GSource) {
    // TODO: needs a bomb to abort on panic
    let source = source as *mut SourceData<T>;
    ptr::read(&(*source).funcs);
    ptr::read(&(*source).data);
}

/// Call prepar on data in GSource
extern "C" fn prepare<T: SourceFuncs>(source: *mut GSource, timeout: *mut c_int) -> c_int {
    let object = source as *mut SourceData<T>;
    let (result, source_timeout) = unsafe { (*object).data.prepare() };
    if let Some(source_timeout) = source_timeout {
        unsafe {
            *timeout = source_timeout as i32;
        }
    }
    bool_to_int(result)
}

fn bool_to_int(boolean: bool) -> c_int {
    if boolean {
        1
    } else {
        0
    }
}
