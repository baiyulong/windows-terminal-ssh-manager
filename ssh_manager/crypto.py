"""Windows DPAPI encryption for securing SSH passwords.

Uses CryptProtectData / CryptUnprotectData, which bind encrypted blobs to
the current Windows user account.  No master password is required.
"""
import base64
import ctypes
import ctypes.wintypes


class _DATA_BLOB(ctypes.Structure):
    _fields_ = [
        ('cbData', ctypes.wintypes.DWORD),
        ('pbData', ctypes.POINTER(ctypes.c_char)),
    ]


def encrypt_password(plaintext: str) -> str:
    """Encrypt *plaintext* with Windows DPAPI and return a base-64 string."""
    data = plaintext.encode('utf-8')
    buf = ctypes.create_string_buffer(data)
    blob_in = _DATA_BLOB(len(data), buf)
    blob_out = _DATA_BLOB()

    ok = ctypes.windll.crypt32.CryptProtectData(
        ctypes.byref(blob_in),
        None, None, None, None,
        0,
        ctypes.byref(blob_out),
    )
    if not ok:
        raise RuntimeError(f'CryptProtectData failed (error {ctypes.GetLastError()})')

    encrypted = ctypes.string_at(blob_out.pbData, blob_out.cbData)
    ctypes.windll.kernel32.LocalFree(blob_out.pbData)
    return base64.b64encode(encrypted).decode('ascii')


def decrypt_password(ciphertext: str) -> str:
    """Decrypt a DPAPI-encrypted *ciphertext* and return the plaintext."""
    data = base64.b64decode(ciphertext)
    buf = ctypes.create_string_buffer(data)
    blob_in = _DATA_BLOB(len(data), buf)
    blob_out = _DATA_BLOB()

    ok = ctypes.windll.crypt32.CryptUnprotectData(
        ctypes.byref(blob_in),
        None, None, None, None,
        0,
        ctypes.byref(blob_out),
    )
    if not ok:
        raise RuntimeError(f'CryptUnprotectData failed (error {ctypes.GetLastError()})')

    decrypted = ctypes.string_at(blob_out.pbData, blob_out.cbData)
    ctypes.windll.kernel32.LocalFree(blob_out.pbData)
    return decrypted.decode('utf-8')
