use console::Term;
use once_cell::sync::Lazy;
use reqwest::StatusCode;
use rustls::ClientSession;
use rustls::ServerCertVerifier;
use std::cell::Cell;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::net::TcpStream;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use url::Url;
use webpki;

use super::keys;

static REMOTE_KEY: Lazy<String> = Lazy::new(|| keys::read_secret("iota.remote.key"));
struct CertificateExtractorServerCertVerifier<SCV: ServerCertVerifier> {
    verifier: SCV,
    tx: Mutex<Cell<Option<Sender<rustls::Certificate>>>>,
}

impl<SCV: ServerCertVerifier> CertificateExtractorServerCertVerifier<SCV> {
    pub fn new(
        verifier: SCV,
        tx: Sender<rustls::Certificate>,
    ) -> CertificateExtractorServerCertVerifier<SCV> {
        CertificateExtractorServerCertVerifier {
            verifier,
            tx: Mutex::new(Cell::new(Some(tx))),
        }
    }
}

impl<SCV: ServerCertVerifier> ServerCertVerifier for CertificateExtractorServerCertVerifier<SCV> {
    #[allow(deprecated)]
    fn verify_server_cert(
        &self,
        roots: &rustls::RootCertStore,
        presented_certs: &[rustls::Certificate],
        dns_name: webpki::DNSNameRef,
        ocsp_response: &[u8],
    ) -> Result<rustls::ServerCertVerified, rustls::TLSError> {
        let tx = self.tx.lock().unwrap().take().unwrap();

        presented_certs
            .iter()
            .for_each(|crt| tx.send(crt.to_owned()).unwrap());

        drop(tx);

        self.verifier
            .verify_server_cert(roots, presented_certs, dns_name, ocsp_response)
    }
}

pub fn download_root_ca_cert_pem(url: &str) -> String {
    let term = Term::stdout();

    let url_parts = Url::parse(url).expect("could not parse url");
    let host_str = url_parts.host_str().expect("no host str");
    let path = &url_parts[url::Position::BeforePath..];

    let (tx, rx) = mpsc::channel();
    let verifier = CertificateExtractorServerCertVerifier::new(rustls::WebPKIVerifier::new(), tx);

    let mut tls_cfg = rustls::ClientConfig::new();
    tls_cfg.root_store =
        rustls_native_certs::load_native_certs().expect("could not load platform certs");
    tls_cfg
        .dangerous()
        .set_certificate_verifier(Arc::new(verifier));

    let server_name = webpki::DNSNameRef::try_from_ascii_str(host_str).unwrap();
    let cfg = Arc::new(tls_cfg);
    let mut conn = ClientSession::new(&cfg, server_name);
    let mut sock = TcpStream::connect(host_str.to_owned() + ":443").unwrap();
    let mut tls = rustls::Stream::new(&mut conn, &mut sock);

    println!("Connecting to file server...");

    write!(
        tls,
        concat!(
            "GET /{} HTTP/1.1\r\n",
            "Host: {}\r\n",
            "Connection: close\r\n",
            "Accept-Encoding: identity\r\n",
            "\r\n"
        ),
        path, host_str
    )
    .unwrap();

    term.clear_last_lines(1).unwrap();
    println!("Downloading certificate...");

    let mut buff = [0];
    tls.read_exact(&mut buff).unwrap();

    let root_cert = rx
        .into_iter()
        .last()
        .expect("No certificates returned by server!");

    term.clear_last_lines(1).unwrap();

    pem::encode(&pem::Pem {
        tag: "CERTIFICATE".to_string(),
        contents: root_cert.0,
    })
}

fn gen_tmp_id() -> String {
    "iota-".to_owned()
        + &SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis()
            .to_string()
}

pub(crate) fn upload_tmp_file(file: std::path::PathBuf) -> String {
    let term = Term::stdout();

    let file = File::open(file).expect("invalid file");
    let id = gen_tmp_id();
    let client = reqwest::blocking::Client::new();

    println!("Authorizing file upload...");

    let put_url = {
        let mut resp = client
            .post("https://hoek.io/api/storage/put-tmp")
            .form(&[("key", &*REMOTE_KEY), ("name", &id)])
            .send()
            .unwrap();

        match resp.status() {
            StatusCode::OK => {}
            _ => panic!("bad HTTP return code: {}", resp.status()),
        }

        let mut buf = vec![];
        resp.read_to_end(&mut buf).unwrap();

        String::from_utf8(buf).unwrap()
    };

    term.clear_last_lines(1).unwrap();
    println!("Uploading file...");

    {
        let resp = client.put(put_url).body(file).send().unwrap();

        match resp.status() {
            StatusCode::OK => {}
            _ => panic!("Bad HTTP return code: {}", resp.status()),
        }
    }

    term.clear_last_lines(1).unwrap();
    println!("Authorizing access to uploaded file...");

    let get_url = {
        let mut resp = client
            .post("https://hoek.io/api/storage/get-tmp")
            .form(&[("key", &*REMOTE_KEY), ("name", &id)])
            .send()
            .unwrap();

        match resp.status() {
            StatusCode::OK => {}
            _ => panic!("bad HTTP return code: {}", resp.status()),
        }

        let mut buf = vec![];
        resp.read_to_end(&mut buf).unwrap();

        String::from_utf8(buf).unwrap()
    };

    term.clear_last_lines(1).unwrap();

    get_url
}
