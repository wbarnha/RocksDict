use crate::encoder::{encode_key, encode_value};
use crate::util::{error_message, to_cpath};
use crate::OptionsPy;
use libc::{self, c_char, size_t};
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::PyResult;
use rocksdb::Options;
use std::ffi::CString;

macro_rules! ffi_try {
    ( $($function:ident)::*() ) => {
        ffi_try_impl!($($function)::*())
    };

    ( $($function:ident)::*( $arg1:expr $(, $arg:expr)* $(,)? ) ) => {
        ffi_try_impl!($($function)::*($arg1 $(, $arg)* ,))
    };
}

macro_rules! ffi_try_impl {
    ( $($function:ident)::*( $($arg:expr,)*) ) => {{
        let mut err: *mut ::libc::c_char = ::std::ptr::null_mut();
        let result = $($function)::*($($arg,)* &mut err);
        if !err.is_null() {
            return Err(PyException::new_err(error_message(err)));
        }
        result
    }};
}

/// SstFileWriter is used to create sst files that can be added to database later
/// All keys in files generated by SstFileWriter will have sequence number = 0.
///
/// Args:
///     options: this options must have the same `raw_mode` as the Rdict DB.
#[pyclass(name = "SstFileWriter")]
#[allow(dead_code)]
pub struct SstFileWriterPy {
    pub(crate) inner: *mut librocksdb_sys::rocksdb_sstfilewriter_t,
    opts: Options,
    dumps: PyObject,
    raw_mode: bool,
}

unsafe impl Send for SstFileWriterPy {}
unsafe impl Sync for SstFileWriterPy {}

struct EnvOptions {
    inner: *mut librocksdb_sys::rocksdb_envoptions_t,
}

impl Drop for EnvOptions {
    fn drop(&mut self) {
        unsafe {
            librocksdb_sys::rocksdb_envoptions_destroy(self.inner);
        }
    }
}

impl Default for EnvOptions {
    fn default() -> Self {
        let opts = unsafe { librocksdb_sys::rocksdb_envoptions_create() };
        Self { inner: opts }
    }
}

#[pymethods]
impl SstFileWriterPy {
    /// Initializes SstFileWriter with given DB options.
    ///
    /// Args:
    ///     options: this options must have the same `raw_mode` as the Rdict DB.
    #[new]
    #[pyo3(signature = (options = OptionsPy::new(false)))]
    fn create(options: OptionsPy, py: Python) -> PyResult<Self> {
        let env_options = EnvOptions::default();
        let raw_mode = options.raw_mode;
        let options = &options.inner_opt;
        let writer = Self::create_raw(options, &env_options);
        let pickle = PyModule::import(py, "pickle")?.to_object(py);
        let pickle_dumps = pickle.getattr(py, "dumps")?;

        Ok(Self {
            inner: writer,
            opts: options.clone(),
            dumps: pickle_dumps,
            raw_mode,
        })
    }

    /// set custom dumps function
    fn set_dumps(&mut self, dumps: PyObject) {
        self.dumps = dumps
    }

    /// Prepare SstFileWriter to write into file located at "file_path".
    fn open(&self, path: &str) -> PyResult<()> {
        let cpath = to_cpath(path)?;
        self.open_raw(&cpath)
    }

    /// Finalize writing to sst file and close file.
    fn finish(&mut self) -> PyResult<()> {
        self.finish_raw()
    }

    /// returns the current file size
    fn file_size(&self) -> u64 {
        self.file_size_raw()
    }

    /// Adds a Put key with value to currently opened file
    /// REQUIRES: key is after any previously added key according to comparator.
    fn __setitem__(&mut self, key: &PyAny, value: &PyAny) -> PyResult<()> {
        let key = encode_key(key, self.raw_mode)?;
        let value = encode_value(value, &self.dumps, self.raw_mode)?;
        self.setitem_raw(&key, &value)
    }

    /// Adds a deletion key to currently opened file
    /// REQUIRES: key is after any previously added key according to comparator.
    fn __delitem__(&mut self, key: &PyAny) -> PyResult<()> {
        let key = encode_key(key, self.raw_mode)?;
        self.delitem_raw(&key)
    }
}

impl SstFileWriterPy {
    #[inline]
    fn create_raw(
        opts: &Options,
        env_opts: &EnvOptions,
    ) -> *mut librocksdb_sys::rocksdb_sstfilewriter_t {
        unsafe { librocksdb_sys::rocksdb_sstfilewriter_create(env_opts.inner, opts.inner()) }
    }

    #[inline]
    fn open_raw(&self, cpath: &CString) -> PyResult<()> {
        unsafe {
            ffi_try!(librocksdb_sys::rocksdb_sstfilewriter_open(
                self.inner,
                cpath.as_ptr() as *const _
            ));

            Ok(())
        }
    }

    #[inline]
    fn finish_raw(&mut self) -> PyResult<()> {
        unsafe {
            ffi_try!(librocksdb_sys::rocksdb_sstfilewriter_finish(self.inner,));
            Ok(())
        }
    }

    #[inline]
    fn file_size_raw(&self) -> u64 {
        let mut file_size: u64 = 0;
        unsafe { librocksdb_sys::rocksdb_sstfilewriter_file_size(self.inner, &mut file_size) };
        file_size
    }

    #[inline]
    fn setitem_raw(&mut self, key: &[u8], value: &[u8]) -> PyResult<()> {
        unsafe {
            ffi_try!(librocksdb_sys::rocksdb_sstfilewriter_put(
                self.inner,
                key.as_ptr() as *const c_char,
                key.len() as size_t,
                value.as_ptr() as *const c_char,
                value.len() as size_t,
            ));
        }
        Ok(())
    }

    #[inline]
    fn delitem_raw(&mut self, key: &[u8]) -> PyResult<()> {
        unsafe {
            ffi_try!(librocksdb_sys::rocksdb_sstfilewriter_delete(
                self.inner,
                key.as_ptr() as *const c_char,
                key.len() as size_t,
            ));
        }
        Ok(())
    }
}

impl Drop for SstFileWriterPy {
    fn drop(&mut self) {
        unsafe {
            librocksdb_sys::rocksdb_sstfilewriter_destroy(self.inner);
        }
    }
}
