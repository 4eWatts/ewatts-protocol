/// Mining pool HTTP server — allows miners to connect and submit shares
/// Run with: ewatts pool serve [port]

use std::io::{Read, Write};
use std::net::TcpListener;

use crate::pool::{MiningPool, Share, register_in_pool, pool_stats};

pub fn serve(port: &str, pool_address: Vec<u8>) {
    // Initialize global pool
    crate::pool::init_global_pool(pool_address);
    
    let addr = format!("0.0.0.0:{}", port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => {
            println!("eWatts Mining Pool");
            println!("  Listen:   http://{}/", addr);
            println!("  Endpoints:");
            println!("    POST /submit    Submit a share (JSON)");
            println!("    POST /register  Register as miner");
            println!("    GET  /stats     Pool statistics");
            println!("    GET  /          Pool dashboard HTML");
            l
        }
        Err(e) => {
            println!("Failed to bind: {}", e);
            return;
        }
    };
    listener.set_nonblocking(true).ok();

    let html = format!("<!DOCTYPE html><html><head><meta charset=\"UTF-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\"><title>eWatts Pool</title><style>body{{font-family:Inter,sans-serif;background:#0b0b12;color:#d4d4dc;padding:20px;max-width:800px;margin:0 auto;}}h1{{color:#fff;}}pre{{background:#10101a;border:1px solid #1e1e32;border-radius:6px;padding:12px;overflow-x:auto;font-family:'JetBrains Mono',monospace;font-size:12px;}}</style></head><body><h1>eWatts Mining Pool</h1><p>Submit shares via <code>POST /submit</code> with JSON body.</p><pre id=\"stats\">Loading...</pre><script>setInterval(async()=>{{let r=await fetch('/stats');let d=await r.json();document.getElementById('stats').textContent=JSON.stringify(d,null,2);}},5000);fetch('/stats').then(r=>r.json()).then(d=>document.getElementById('stats').textContent=JSON.stringify(d,null,2));</script></body></html>");

    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut buf = [0u8; 4096];
                let n = match stream.read(&mut buf) {
                    Ok(n) => n,
                    Err(_) => continue,
                };
                let request = String::from_utf8_lossy(&buf[..n]).to_string();

                let response = if request.starts_with("POST /submit") {
                    let body_start = request.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
                    let body = &request[body_start..];
                    match serde_json::from_str::<Share>(body) {
                        Ok(share) => {
                            let is_block = crate::pool::submit_share_to_pool(share);
                            if is_block {
                                json_response(200, "{\"status\":\"block_found\",\"message\":\"Valid block!\"}")
                            } else {
                                json_response(200, "{\"status\":\"accepted\",\"message\":\"Share accepted\"}")
                            }
                        }
                        Err(e) => json_response(400, &format!("{{\"error\":\"Invalid share: {}\"}}", e)),
                    }
                } else if request.starts_with("POST /register") {
                    let body_start = request.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
                    let body = &request[body_start..];
                    match serde_json::from_str::<serde_json::Value>(body) {
                        Ok(v) => {
                            let id_hex = v["miner_id"].as_str().unwrap_or("");
                            let addr_hex = v["address"].as_str().unwrap_or("");
                            if id_hex.len() == 64 && addr_hex.len() == 64 {
                                let id = hex::decode(id_hex).unwrap_or_default();
                                let addr = hex::decode(addr_hex).unwrap_or_default();
                                if id.len() == 32 && addr.len() == 32 {
                                    let mut id_arr = [0u8; 32];
                                    let mut addr_arr = [0u8; 32];
                                    id_arr.copy_from_slice(&id);
                                    addr_arr.copy_from_slice(&addr);
                                    register_in_pool(id_arr, addr_arr.to_vec());
                                    json_response(200, "{\"status\":\"registered\"}")
                                } else {
                                    json_response(400, "{\"error\":\"Invalid key length\"}")
                                }
                            } else {
                                json_response(400, "{\"error\":\"Invalid hex length (need 64 chars)\"}")
                            }
                        }
                        Err(e) => json_response(400, &format!("{{\"error\":\"Invalid JSON: {}\"}}", e)),
                    }
                } else if request.starts_with("GET /stats") {
                    let stats = pool_stats();
                    json_response(200, &stats.to_string())
                } else {
                    // Serve dashboard HTML
                    format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}", html.len(), html)
                };
                let _ = stream.write_all(response.as_bytes());
            }
            Err(_) => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
}

fn json_response(code: u16, body: &str) -> String {
    format!("HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}",
        code, if code == 200 { "OK" } else { "Error" }, body.len(), body)
}
