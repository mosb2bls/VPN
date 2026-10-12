//! Windows Filtering Platform kill switch (ADR-0007). Applies the pure plan
//! from `edge_core::wfp` in one transaction, in our own sublayer, inside a
//! **dynamic** session: "any objects added during the session are
//! automatically deleted when the session ends" (Microsoft Learn,
//! FWPM_SESSION0). Releasing = closing the engine handle; if the helper
//! process dies, Windows removes the filters itself.

use std::ffi::c_void;
use std::path::Path;

use edge_core::wfp::{Family, FilterSpec, Match, v4_mask};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::*;
use windows_sys::core::GUID;

use crate::wide;

/// `RPC_C_AUTHN_WINNT`.
const RPC_C_AUTHN_WINNT: u32 = 10;
/// Our sublayer (fixed so it is recognisable in `netsh wfp show filters`).
const SUBLAYER: GUID = GUID::from_u128(0x6d0c3c3e_8f5a_4b8e_9f43_0e5d9b6a7c21);

fn check(what: &str, rc: u32) -> Result<(), String> {
    if rc == 0 { Ok(()) } else { Err(format!("{what} failed (0x{rc:08x})")) }
}

/// An engaged kill switch. Dropping it (or the process ending) removes it.
pub struct KillSwitch {
    engine: HANDLE,
}

// The engine handle is used from one thread at a time (behind a Mutex).
unsafe impl Send for KillSwitch {}

impl Drop for KillSwitch {
    fn drop(&mut self) {
        unsafe { FwpmEngineClose0(self.engine) };
    }
}

fn display(name: &mut Vec<u16>) -> FWPM_DISPLAY_DATA0 {
    FWPM_DISPLAY_DATA0 { name: name.as_mut_ptr(), description: std::ptr::null_mut() }
}

/// Engage: block outbound connects except what `plan` permits. `core_exe`
/// is the installed sing-box (permitted by app ID).
pub fn engage(plan: &[FilterSpec], core_exe: &Path) -> Result<KillSwitch, String> {
    let mut session: FWPM_SESSION0 = unsafe { std::mem::zeroed() };
    session.flags = FWPM_SESSION_FLAG_DYNAMIC;
    let mut sname = wide(&format!("{} kill switch", edge_core::APP_NAME));
    session.displayData = display(&mut sname);
    let mut engine: HANDLE = std::ptr::null_mut();
    check("FwpmEngineOpen0", unsafe {
        FwpmEngineOpen0(std::ptr::null(), RPC_C_AUTHN_WINNT, std::ptr::null(), &session, &mut engine)
    })?;
    let ks = KillSwitch { engine };

    let exe = wide(&core_exe.to_string_lossy());
    let mut app_id: *mut FWP_BYTE_BLOB = std::ptr::null_mut();
    check("FwpmGetAppIdFromFileName0", unsafe { FwpmGetAppIdFromFileName0(exe.as_ptr(), &mut app_id) })?;
    let result = (|| {
        check("FwpmTransactionBegin0", unsafe { FwpmTransactionBegin0(engine, 0) })?;
        let r = add_all(engine, plan, app_id);
        if r.is_err() {
            unsafe { FwpmTransactionAbort0(engine) };
            return r;
        }
        check("FwpmTransactionCommit0", unsafe { FwpmTransactionCommit0(engine) })
    })();
    unsafe { FwpmFreeMemory0(&mut app_id as *mut _ as *mut *mut c_void) };
    result.map(|_| ks)
}

fn add_all(engine: HANDLE, plan: &[FilterSpec], app_id: *mut FWP_BYTE_BLOB) -> Result<(), String> {
    let mut lname = wide(&format!("{} kill switch", edge_core::APP_NAME));
    let mut sub: FWPM_SUBLAYER0 = unsafe { std::mem::zeroed() };
    sub.subLayerKey = SUBLAYER;
    sub.displayData = display(&mut lname);
    sub.weight = 0xffff;
    check("FwpmSubLayerAdd0", unsafe { FwpmSubLayerAdd0(engine, &sub, std::ptr::null_mut()) })?;
    for spec in plan {
        add_filter(engine, spec, app_id)?;
    }
    Ok(())
}

fn add_filter(engine: HANDLE, spec: &FilterSpec, app_id: *mut FWP_BYTE_BLOB) -> Result<(), String> {
    // Address masks are passed by pointer: keep them alive until FwpmFilterAdd0.
    let mut v4mask = match &spec.matches {
        Match::RemoteV4 { addr, prefix } => FWP_V4_ADDR_AND_MASK { addr: u32::from(*addr), mask: v4_mask(*prefix) },
        _ => FWP_V4_ADDR_AND_MASK { addr: 0, mask: 0 },
    };
    let mut v6mask = match &spec.matches {
        Match::RemoteV6 { addr, prefix } => FWP_V6_ADDR_AND_MASK { addr: addr.octets(), prefixLength: *prefix },
        _ => FWP_V6_ADDR_AND_MASK { addr: [0; 16], prefixLength: 0 },
    };
    let mut conds: Vec<FWPM_FILTER_CONDITION0> = Vec::new();
    let mut cond = |field: GUID, match_type: FWP_MATCH_TYPE, ty: FWP_DATA_TYPE, value: FWP_CONDITION_VALUE0_0| {
        conds.push(FWPM_FILTER_CONDITION0 {
            fieldKey: field,
            matchType: match_type,
            conditionValue: FWP_CONDITION_VALUE0 { r#type: ty, Anonymous: value },
        });
    };
    match &spec.matches {
        Match::All => {}
        Match::Loopback => cond(
            FWPM_CONDITION_FLAGS,
            FWP_MATCH_FLAGS_ALL_SET,
            FWP_UINT32,
            FWP_CONDITION_VALUE0_0 { uint32: FWP_CONDITION_FLAG_IS_LOOPBACK },
        ),
        Match::CoreApp => cond(FWPM_CONDITION_ALE_APP_ID, FWP_MATCH_EQUAL, FWP_BYTE_BLOB_TYPE, FWP_CONDITION_VALUE0_0 { byteBlob: app_id }),
        Match::Udp { local_port, remote_port } => {
            cond(FWPM_CONDITION_IP_PROTOCOL, FWP_MATCH_EQUAL, FWP_UINT8, FWP_CONDITION_VALUE0_0 { uint8: 17 });
            cond(FWPM_CONDITION_IP_LOCAL_PORT, FWP_MATCH_EQUAL, FWP_UINT16, FWP_CONDITION_VALUE0_0 { uint16: *local_port });
            cond(FWPM_CONDITION_IP_REMOTE_PORT, FWP_MATCH_EQUAL, FWP_UINT16, FWP_CONDITION_VALUE0_0 { uint16: *remote_port });
        }
        Match::RemoteV4 { .. } => {
            cond(FWPM_CONDITION_IP_REMOTE_ADDRESS, FWP_MATCH_EQUAL, FWP_V4_ADDR_MASK, FWP_CONDITION_VALUE0_0 { v4AddrMask: &mut v4mask });
        }
        Match::RemoteV6 { .. } => {
            cond(FWPM_CONDITION_IP_REMOTE_ADDRESS, FWP_MATCH_EQUAL, FWP_V6_ADDR_MASK, FWP_CONDITION_VALUE0_0 { v6AddrMask: &mut v6mask });
        }
    }
    let mut name = wide(&format!("{} kill switch {:?}", edge_core::APP_NAME, spec.matches).replace(['{', '}'], ""));
    let mut f: FWPM_FILTER0 = unsafe { std::mem::zeroed() };
    f.displayData = display(&mut name);
    f.layerKey = match spec.family {
        Family::V4 => FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        Family::V6 => FWPM_LAYER_ALE_AUTH_CONNECT_V6,
    };
    f.subLayerKey = SUBLAYER;
    f.weight = FWP_VALUE0 { r#type: FWP_UINT8, Anonymous: FWP_VALUE0_0 { uint8: spec.weight } };
    f.numFilterConditions = conds.len() as u32;
    f.filterCondition = if conds.is_empty() { std::ptr::null_mut() } else { conds.as_mut_ptr() };
    f.action.r#type = if spec.permit { FWP_ACTION_PERMIT } else { FWP_ACTION_BLOCK };
    let mut id = 0u64;
    check("FwpmFilterAdd0", unsafe { FwpmFilterAdd0(engine, &f, std::ptr::null_mut(), &mut id) })
}
