//! Ewatts Wallet — reference testnet implementation (NOT production-hardened).

use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::ThreadRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::block::*;
use crate::privacy::*;
use crate::state::{UtxoEntry, UtxoKey, UtxoSet};

const WALLET_DIR: &str = "ewatts_data/wallets";

/// A single stealth keypair in the wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthKeyEntry {
    pub view_secret: [u8; 32],
    pub spend_secret: [u8; 32],
    pub spend_key: [u8; 32],
    pub view_key: [u8; 32],
    pub legacy_public_key: Vec<u8>,
    pub label: String,
}

impl StealthKeyEntry {
    pub fn address(&self) -> String {
        hex::encode(self.spend_key)
    }

    pub fn stealth_address(&self) -> Result<StealthAddress, String> {
        let s = curve25519_dalek::ristretto::CompressedRistretto(self.spend_key)
            .decompress()
            .ok_or_else(|| "Invalid spend key in wallet".to_string())?;
        let v = curve25519_dalek::ristretto::CompressedRistretto(self.view_key)
            .decompress()
            .ok_or_else(|| "Invalid view key in wallet".to_string())?;
        Ok(StealthAddress {
            spend_key: s,
            view_key: v,
        })
    }

    /// Get the private scalars.
    pub fn secrets(&self) -> (Scalar, Scalar) {
        let view = Scalar::from_bytes_mod_order(self.view_secret);
        let spend = Scalar::from_bytes_mod_order(self.spend_secret);
        (view, spend)
    }

    /// Get ed25519 verifying key for legacy UTXO detection (P1-3 fix).
    pub fn legacy_verifying_key(&self) -> Option<VerifyingKey> {
        if self.legacy_public_key.len() == 32 {
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&self.legacy_public_key[..32]);
            VerifyingKey::from_bytes(&bytes).ok()
        } else {
            None
        }
    }
}

/// A UTXO owned by this wallet.
#[derive(Debug, Clone)]
pub struct OwnedUtxo {
    pub key: UtxoKey,
    pub entry: UtxoEntry,
    pub one_time_key: Scalar, // derived private key for spending
    pub commitment_val: u64,  // amount
}

/// Wallet state: loaded keys.
pub struct Wallet {
    pub keys: Vec<StealthKeyEntry>,
}

impl Wallet {
    /// Load or initialize the wallet from disk.
    pub fn load() -> Self {
        let path = format!("{}/keys.json", WALLET_DIR);
        let keys = if Path::new(&path).exists() {
            let data = fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };
        Wallet { keys }
    }

    /// Save keys to disk.
    pub fn save(&self) {
        let path = format!("{}/keys.json", WALLET_DIR);
        fs::create_dir_all(WALLET_DIR).ok();
        let data = serde_json::to_string_pretty(&self.keys).unwrap();
        fs::write(&path, &data).ok();
        println!("  Wallet saved: {}", path);
    }

    pub fn new_key(&mut self, label: &str) {
        let mut rng = rand::thread_rng();
        let (addr, key) = StealthAddress::generate(&mut rng);
        let ed_secret = SigningKey::generate(&mut rng);
        let ed_public = ed_secret.verifying_key().to_bytes().to_vec();
        let entry = StealthKeyEntry {
            view_secret: key.view.to_bytes(),
            spend_secret: key.spend.to_bytes(),
            spend_key: addr.spend_key.compress().to_bytes(),
            view_key: addr.view_key.compress().to_bytes(),
            legacy_public_key: ed_public,
            label: label.to_string(),
        };
        let addr_hex = entry.address();
        self.keys.push(entry);
        self.save();
        println!("  Generated stealth key: {}", &addr_hex[..16]);
        println!("  Label: {}", label);
    }

    /// Check UTXO set for outputs this wallet can spend.
    pub fn scan_utxos(&self, utxo_set: &UtxoSet) -> Vec<OwnedUtxo> {
        let mut owned = Vec::new();
        let map = utxo_set.utxos_map();
        for (key, entry) in map.iter() {
            if let Some(sd) = &entry.stealth_dest {
                if let Some(eph) = &entry.ephemeral {
                    let ephem_point =
                        match curve25519_dalek::ristretto::CompressedRistretto(*eph).decompress() {
                            Some(p) => p,
                            None => continue, // malformed ephemeral, skip
                        };
                    for k in &self.keys {
                        let (view, spend) = k.secrets();
                        let derived =
                            crate::privacy::recover_one_time_key(&view, &spend, &ephem_point);
                        let expected_dest = derived * ring_g();
                        let actual_dest = match curve25519_dalek::ristretto::CompressedRistretto(
                            *sd,
                        )
                        .decompress()
                        {
                            Some(p) => p,
                            None => continue,
                        };
                        if expected_dest == actual_dest {
                            owned.push(OwnedUtxo {
                                key: key.clone(),
                                entry: entry.clone(),
                                one_time_key: derived,
                                commitment_val: entry.amount,
                            });
                            break;
                        }
                    }
                }
            }
            // Also check legacy (ed25519) public keys — SKIP if stealth already matched
            let stealth_matched = owned.last().map(|o| o.key == *key).unwrap_or(false);
            if !stealth_matched {
                for k in &self.keys {
                if let Some(vk) = k.legacy_verifying_key() {
                    let pk_hash = crate::block::TxOutput::hash_pubkey(&vk.to_bytes());
                    if entry.pubkey_hash != [0u8; 20] && entry.pubkey_hash == pk_hash {
                        owned.push(OwnedUtxo {
                            key: key.clone(),
                            entry: entry.clone(),
                            one_time_key: Scalar::from(0u64), // placeholder, legacy mode
                            commitment_val: entry.amount,
                        });
                        break;
                    }
                }
            }
        }
    }
    owned
    }

    /// List all wallet keys.
    pub fn list(&self) {
        if self.keys.is_empty() {
            println!("  No keys in wallet. Run 'wallet new' first.");
            return;
        }
        for (i, k) in self.keys.iter().enumerate() {
            println!("  [{:02}] {}  ({})", i, k.address(), k.label);
        }
    }
}

/// Create a private transaction using the first wallet key.
/// This is a simplified version — real wallet needs proper ring selection and R storage.
pub fn create_private_tx(
    wallet: &Wallet,
    to_addr: &StealthAddress,
    amount: u64,
    utxo_set: &UtxoSet,
    rng: &mut ThreadRng,
) -> Result<Transaction, String> {
    if wallet.keys.is_empty() {
        return Err("No wallet keys".into());
    }
    if amount == 0 {
        return Err("Cannot send zero amount".into()); // P2-1
    }
    let key = &wallet.keys[0];
    let addr = key
        .stealth_address()
        .map_err(|e| format!("Invalid wallet address: {}", e))?;
    let (_view_sec, _spend_sec) = key.secrets();

    // Scan for own UTXOs
    let owned = wallet.scan_utxos(utxo_set);

    // Select UTXOs to spend
    let mut total_available = 0u64;
    for o in &owned {
        total_available += o.entry.amount;
    }
    if total_available < amount {
        return Err(format!(
            "Insufficient balance: have {}, need {}",
            total_available, amount
        ));
    }

    let mut total = 0u64;
    let mut selected_utxos = Vec::new();
    for o in &owned {
        if total >= amount {
            break;
        }
        total += o.entry.amount;
        selected_utxos.push(o.clone());
    }

    // Get all UTXOs for ring member selection (P1-2: filter to stealth-only)
    let all_utxos: Vec<(UtxoKey, UtxoEntry)> = utxo_set
        .utxos_map()
        .iter()
        .filter(|(_, v)| v.stealth_dest.is_some())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let ring_size = 11usize;
    let mut ring_members = Vec::with_capacity(selected_utxos.len());
    let mut ring_pubkeys: Vec<Vec<RistrettoPoint>> = Vec::with_capacity(selected_utxos.len());
    let mut selected_inputs = Vec::with_capacity(selected_utxos.len());
    let mut secret_keys = Vec::with_capacity(selected_utxos.len());

    // Decide real_index ONCE: all inputs share the same position in their rings
    let real_index = rng.next_u32() as usize % ring_size;

    // First pass: build rings, compute key_images BEFORE signing (P0-2 fix)
    for utxo in &selected_utxos {
        // Select ring members: filter FIRST, then take (P0-4 fix)
        let mut members: Vec<UtxoRef> = Vec::with_capacity(ring_size);
        let mut indices: Vec<usize> = (0..all_utxos.len()).collect();
        for i in (1..indices.len()).rev() {
            let j = rng.next_u32() as usize % (i + 1);
            indices.swap(i, j);
        }
        // Pick decoys (filter out own UTXO)
        for &idx in indices
            .iter()
            .filter(|&&i| all_utxos[i].0 != utxo.key)
            .take(ring_size - 1)
        {
            members.push(UtxoRef {
                tx_hash: all_utxos[idx].0.tx_hash,
                output_index: all_utxos[idx].0.output_index,
            });
        }
        // Insert own UTXO at the FIXED real_index (shared across all layers)
        // Safeguard: if there aren't enough decoys, insert at end instead
        let insert_pos = real_index.min(members.len());
        members.insert(
            insert_pos,
            UtxoRef {
                tx_hash: utxo.key.tx_hash,
                output_index: utxo.key.output_index,
            },
        );

        // Build ring pubkeys for this layer
        let mut layer_ring: Vec<RistrettoPoint> = Vec::with_capacity(members.len());
        for m in &members {
            if let Some(entry) = utxo_set.utxos_map().get(&UtxoKey {
                tx_hash: m.tx_hash,
                output_index: m.output_index,
            }) {
                let pk = entry
                    .stealth_dest_point()
                    .ok_or_else(|| "Ring member missing stealth dest".to_string())?;
                layer_ring.push(pk);
            } else {
                return Err(format!("Ring member UTXO not found: {:?}", m));
            }
        }
        ring_pubkeys.push(layer_ring);

        // Compute key_image deterministically (P0-2 fix: was placeholder, set after sign)
        let key_pubkey = ring_pubkeys.last().unwrap()[real_index];
        let key_image = utxo.one_time_key * hash_pk(&key_pubkey);

        selected_inputs.push(TxInput {
            previous_tx_hash: utxo.key.tx_hash,
            output_index: utxo.key.output_index,
            key_image: key_image.compress().to_bytes(),
            revealed_pubkey: vec![],
        });
        secret_keys.push(utxo.one_time_key);
        ring_members.push(members);
    }

    // Destination: create stealth output for recipient
    let (dest, _r_ephem) = to_addr.derive_destination(rng);

    // Output to recipient — use prove_with_blinding (P0-1 fix)
    let mut outputs = Vec::new();
    let (range_to, total_blinding_to) = RangeProof::prove_with_blinding(amount, 32, rng);
    let comm_to = Commitment::new_with_blinding(amount, total_blinding_to);
    outputs.push(TxOutput::new_private(
        amount,
        dest.dest.compress().to_bytes(),
        comm_to.0.compress().to_bytes(),
        serde_json::to_vec(&range_to).unwrap_or_default(),
    ));
    if let Some(o) = outputs.last_mut() {
        o.ephemeral = Some(dest.ephemeral.compress().to_bytes());
    }

    // Change output to self
    if total > amount {
        let change = total - amount;
        let change_dest = StealthAddress {
            spend_key: addr.spend_key,
            view_key: addr.view_key,
        };
        let (c_dest, _) = change_dest.derive_destination(rng);
        let (range_ch, tot_blinding_ch) = RangeProof::prove_with_blinding(change, 32, rng);
        let comm_ch = Commitment::new_with_blinding(change, tot_blinding_ch);
        outputs.push(TxOutput::new_private(
            change,
            c_dest.dest.compress().to_bytes(),
            comm_ch.0.compress().to_bytes(),
            serde_json::to_vec(&range_ch).unwrap_or_default(),
        ));
        if let Some(o) = outputs.last_mut() {
            o.ephemeral = Some(c_dest.ephemeral.compress().to_bytes());
        }
    }

    // Transpose ring_pubkeys: MLSAG needs [ring_pos][layer]
    if ring_pubkeys.is_empty() {
        return Err("No ring pubkeys".into());
    }
    let n_layers = ring_pubkeys.len();
    let ring_sz = ring_pubkeys[0].len();
    let mut mlsag_ring = vec![Vec::with_capacity(n_layers); ring_sz];
    for pos in 0..ring_sz {
        for layer in 0..n_layers {
            mlsag_ring[pos].push(ring_pubkeys[layer][pos]);
        }
    }

    // Build transaction with finalized inputs (key_images already set, P0-2)
    let tx = Transaction {
        version: 1,
        inputs: selected_inputs,
        outputs,
        ring_size: ring_size as u16,
        signatures: vec![],
        mlsag: None,
        ring_members: Some(ring_members),
    };

    // Sign over finalized tx_msg (includes key_images in hash, P0-2)
    // Using the same real_index that was used for ring construction
    let msg = crate::state::tx_msg(&tx);
    let sig = MLSAGSignature::sign(&mlsag_ring, &secret_keys, real_index, &msg, rng);

    let mut tx = tx;
    tx.mlsag = Some(MlsagData::from_sig(&sig));
    Ok(tx)
}

// ─── Seed Phrase (BIP39-style mnemonic) ────────────────────────────────

/// BIP39 English wordlist (first 20 words for reference, full list embedded).
const BIP39_WORDS: &[&str] = &[
    "abandon","ability","able","about","above","absent","absorb","abstract","absurd","abuse",
    "access","accident","account","accuse","achieve","acid","acoustic","acquire","across","act",
    "action","actor","actress","actual","adapt","add","addict","address","adjust","admit",
    "adult","advance","advice","aerobic","affair","afford","afraid","again","age","agent",
    "agree","ahead","aim","air","airport","aisle","alarm","album","alcohol","alert",
    "alien","all","alley","allow","almost","alone","alpha","already","also","alter",
    "always","amateur","amazing","among","amount","amused","analyst","anchor","ancient","anger",
    "angle","angry","animal","ankle","announce","annual","another","answer","antenna","antique",
    "anxiety","any","apart","apology","appear","apple","approve","april","arch","arctic",
    "area","arena","argue","arm","armed","armor","army","around","arrange","arrest",
    "arrive","arrow","art","artefact","artist","artwork","ask","aspect","assault","asset",
    "assist","assume","asthma","athlete","atom","attack","attend","attitude","attract","auction",
    "audit","august","aunt","author","auto","autumn","average","avocado","avoid","awake",
    "aware","away","awesome","awful","awkward","axis","baby","bachelor","bacon","badge",
    "bag","balance","balcony","ball","bamboo","banana","banner","bar","barely","bargain",
    "barrel","base","basic","basket","battle","beach","bean","beauty","because","become",
    "beef","before","begin","behavior","behind","believe","below","belt","bench","benefit",
    "best","betray","better","between","beyond","bicycle","bid","bike","bind","biology",
    "bird","birth","bitter","black","blade","blame","blanket","blast","bleak","bless",
    "blind","blood","blossom","blouse","blue","blur","blush","board","boat","body",
    "boil","bomb","bone","bonus","book","boost","border","boring","borrow","boss",
    "bottom","bounce","box","boy","bracket","brain","brand","brass","brave","bread",
    "breeze","brick","bridge","brief","bright","bring","brisk","broccoli","broken","bronze",
    "broom","brother","brown","brush","bubble","buddy","budget","buffalo","build","bulb",
    "bulk","bullet","bundle","bunker","burden","burger","burst","bus","business","busy",
    "butter","buyer","buzz","cabbage","cabin","cable","cactus","cage","cake","call",
    "calm","camera","camp","can","canal","cancel","candle","cannon","canoe","canvas",
    "canyon","capable","capital","captain","car","carbon","card","cargo","carpet","carry",
    "cart","case","cash","casino","castle","casual","cat","catalog","catch","category",
    "cattle","caught","cause","caution","cave","ceiling","celery","cement","census","century",
    "cereal","certain","chair","chalk","champion","change","chaos","chapter","charge","chase",
    "chat","cheap","check","cheese","chef","cherry","chest","chicken","chief","child",
    "chimney","choice","choose","chronic","chuckle","chunk","churn","cigar","cinnamon","circle",
    "citizen","city","civil","claim","clap","clarify","claw","clay","clean","clerk",
    "clever","click","client","cliff","climb","clinic","clip","clock","clog","close",
    "cloth","cloud","clown","club","clump","cluster","clutch","coach","coast","coconut",
    "code","coffee","coil","coin","collect","color","column","combine","come","comfort",
    "comic","common","company","concert","conduct","confirm","congress","connect","consider","control",
    "convince","cook","cool","copper","copy","coral","core","corn","correct","cost",
    "cotton","couch","country","couple","course","cousin","cover","coyote","crack","cradle",
    "craft","cram","crane","crash","crater","crawl","crazy","cream","credit","creek",
    "crew","cricket","crime","crisp","critic","crop","cross","crouch","crowd","crucial",
    "cruel","cruise","crumble","crunch","crush","cry","crystal","cube","culture","cup",
    "cupboard","curious","current","curtain","curve","cushion","custom","cute","cycle","dad",
    "damage","damp","dance","danger","daring","darken","dash","date","daughter","dawn",
    "day","deal","debate","debris","decade","december","decide","decline","decorate","decrease",
    "deer","defense","define","defy","degree","delay","deliver","demand","demise","denial",
    "dentist","deny","depart","depend","deposit","depth","deputy","derive","describe","desert",
    "design","desk","despair","destroy","detail","detect","develop","device","devote","diagram",
    "dial","diamond","diary","dice","diesel","diet","differ","digital","dignity","dilemma",
    "dinner","dinosaur","direct","dirt","disagree","discover","disease","dish","dismiss","disorder",
    "display","distance","divert","divide","divorce","dizzy","doctor","document","dog","doll",
    "dolphin","domain","donate","donkey","donor","door","dose","double","dove","draft",
    "dragon","drama","drastic","draw","dream","dress","drift","drill","drink","drip",
    "drive","drop","drum","dry","duck","dumb","dune","during","dust","dutch",
    "duty","dwarf","dynamic","eager","eagle","early","earn","earth","easily","east",
    "easy","echo","ecology","economy","edge","edit","educate","effort","egg","eight",
    "either","elbow","elder","electric","elegant","element","elephant","elevator","elite","else",
    "embark","embody","embrace","emerge","emotion","employ","empower","empty","enable","enact",
    "end","endless","endorse","enemy","energy","enforce","engage","engine","enhance","enjoy",
    "enlist","enough","enrich","enroll","ensure","enter","entire","entry","envelope","episode",
    "equal","equip","era","erase","erode","erosion","error","erupt","escape","essay",
    "essence","estate","eternal","ethics","evidence","evil","evoke","evolve","exact","example",
    "exceed","exchange","exclude","execute","exercise","exhaust","exhibit","exile","exist","exit",
    "exotic","expand","expect","expire","explain","expose","express","extend","extra","eye",
    "eyebrow","fabric","face","faculty","fade","faint","faith","fall","false","fame",
    "family","famous","fan","fancy","fantasy","farm","fashion","fat","fatal","father",
    "fatigue","fault","favorite","feature","february","federal","fee","feed","feel","female",
    "fence","festival","fetch","fever","few","fiber","fiction","field","figure","file",
    "film","filter","final","find","fine","finger","finish","fire","firm","first",
    "fiscal","fish","fit","fitness","fix","flag","flame","flash","flat","flavor",
    "flee","flight","flip","float","flock","floor","flower","fluid","flush","fly",
    "foam","focus","fog","foil","fold","follow","food","foot","force","foreign",
    "forest","forget","fork","fortune","forum","forward","fossil","foster","found","fox",
    "fragile","frame","frequent","fresh","friend","fringe","frog","front","frost","frown",
    "frozen","fruit","fuel","fun","funny","furnace","fury","future","gadget","gain",
    "galaxy","gallery","game","gap","garage","garbage","garden","garlic","garment","gas",
    "gasp","gate","gather","gauge","gaze","general","genius","genre","gentle","genuine",
    "gesture","ghost","giant","gift","giggle","ginger","giraffe","girl","give","glad",
    "glance","glare","glass","glide","glimpse","globe","gloom","glory","glove","glow",
    "glue","goat","goddess","gold","good","goose","gorilla","gospel","gossip","govern",
    "gown","grab","grace","grain","grant","grape","grass","gravity","great","green",
    "grid","grief","grit","grocery","group","grow","grunt","guard","guess","guide",
    "guilt","guitar","gun","gym","habit","hair","half","hammer","hamster","hand",
    "happy","harbor","hard","harsh","harvest","hat","have","hawk","hazard","head",
    "health","heart","heavy","hedgehog","height","hello","helmet","help","hen","hero",
    "hidden","high","hill","hint","hip","hire","history","hobby","hockey","hold",
    "hole","holiday","hollow","home","honey","hood","hope","horn","horror","horse",
    "hospital","host","hotel","hour","hover","hub","huge","human","humble","humor",
    "hundred","hungry","hunt","hurdle","hurry","hurt","husband","hybrid","ice","icon",
    "idea","identify","idle","ignore","ill","illegal","illness","image","imitate","immense",
    "immune","impact","impose","improve","impulse","inch","include","income","increase","index",
    "indicate","indoor","industry","infant","inflict","inform","inhale","inherit","initial","inject",
    "injury","inmate","inner","innocent","input","inquiry","insane","insect","inside","inspire",
    "install","intact","interest","into","invest","invite","involve","iron","island","isolate",
    "issue","item","ivory","jacket","jaguar","jar","jazz","jealous","jeans","jelly",
    "jewel","job","join","joke","journey","joy","judge","juice","jump","jungle",
    "junior","junk","just","kangaroo","keen","keep","ketchup","key","kick","kid",
    "kidney","kind","kingdom","kiss","kit","kitchen","kite","kitten","kiwi","knee",
    "knife","knock","know","lab","label","labor","ladder","lady","lake","lamp",
    "language","laptop","large","later","latin","laugh","laundry","lava","law","lawn",
    "lawsuit","layer","lazy","leader","leaf","learn","leave","lecture","left","leg",
    "legal","legend","leisure","lemon","lend","length","lens","leopard","lesson","letter",
    "level","liar","liberty","library","license","life","lift","light","like","limb",
    "limit","link","lion","liquid","list","little","live","lizard","load","loan",
    "lobster","local","lock","logic","lonely","long","loop","lottery","loud","lounge",
    "love","loyal","lucky","luggage","lumber","lunar","lunch","luxury","lyrics","machine",
    "mad","magic","magnet","maid","mail","main","major","make","mammal","man",
    "manage","mandate","mango","mansion","manual","maple","marble","march","margin","marine",
    "market","marriage","mask","mass","master","match","material","math","matrix","matter",
    "maximum","maze","meadow","mean","measure","meat","mechanic","medal","media","melody",
    "melt","member","memory","mention","menu","mercy","merge","merit","merry","mesh",
    "message","metal","method","middle","midnight","milk","million","mimic","mind","minimum",
    "minor","minute","miracle","mirror","misery","miss","mistake","mix","mixed","mixture",
    "mobile","model","modify","mom","moment","monitor","monkey","monster","month","moon",
    "moral","more","morning","mosquito","mother","motion","motor","mountain","mouse","move",
    "movie","much","muffin","mule","multiply","muscle","museum","mushroom","music","must",
    "mutual","myself","mystery","myth","naive","name","napkin","narrow","nasty","nation",
    "nature","near","neck","need","negative","neglect","neither","nephew","nerve","nest",
    "net","network","neutral","never","news","next","nice","night","noble","noise",
    "nominee","noodle","normal","north","nose","notable","note","nothing","notice","novel",
    "now","nuclear","number","nurse","nut","oak","obey","object","oblige","obscure",
    "observe","obtain","obvious","occur","ocean","october","odor","off","offer","office",
    "often","oil","okay","old","olive","olympic","omit","once","one","onion",
    "online","only","open","opera","opinion","oppose","option","orange","orbit","orchard",
    "order","ordinary","organ","orient","original","orphan","ostrich","other","outdoor","outer",
    "output","outside","oval","oven","over","own","owner","oxygen","oyster","ozone",
    "pact","paddle","page","pair","palace","palm","panda","panel","panic","panther",
    "paper","parade","parent","park","parrot","party","pass","patch","path","patient",
    "patrol","pattern","pause","pave","payment","peace","peanut","pear","peasant","pelican",
    "pen","penalty","pencil","people","pepper","perfect","permit","person","pet","phone",
    "photo","phrase","physical","piano","picnic","picture","piece","pig","pigeon","pill",
    "pilot","pink","pioneer","pipe","pistol","pitch","pizza","place","planet","plastic",
    "plate","play","player","pleasure","plenty","plot","pluck","plug","plunge","poem",
    "poet","point","polar","pole","police","pond","pony","pool","popular","portion",
    "position","possible","post","potato","pottery","poverty","powder","power","practice","praise",
    "predict","prefer","prepare","present","pretty","prevent","price","pride","primary","print",
    "priority","prison","private","prize","problem","process","produce","profit","program","project",
    "property","proposal","protect","prove","provide","public","pudding","pull","pulp","pulse",
    "pumpkin","punch","pupil","puppy","purchase","purity","purpose","purse","push","put",
    "puzzle","pyramid","quality","quantum","quarter","question","quick","quit","quiz","quote",
    "rabbit","raccoon","race","rack","radar","radio","rail","rain","raise","rally",
    "ramp","ranch","random","range","rapid","rare","rate","rather","raven","raw",
    "razor","ready","real","reason","rebel","rebuild","recall","receive","recipe","record",
    "recycle","reduce","reflect","reform","refuse","region","regret","regular","reject","relax",
    "release","relief","rely","remain","remember","remind","remove","render","renew","rent",
    "reopen","repair","repeat","replace","report","require","rescue","resemble","resist","resource",
    "response","result","retire","retreat","return","reunion","reveal","review","reward","rhythm",
    "rib","ribbon","rice","rich","ride","ridge","rifle","right","rigid","ring",
    "riot","ripple","risk","ritual","rival","river","road","roast","robot","robust",
    "rocket","romance","roof","rookie","room","rose","rotate","rough","round","route",
    "royal","rubber","rude","rug","rule","run","runway","rural","saddle","sadness",
    "safe","sail","salad","salmon","salon","salt","salute","same","sample","sand",
    "satisfy","satoshi","sauce","sausage","save","scale","scan","scare","scatter","scene",
    "scheme","school","science","scissors","scorpion","scout","scrap","screen","script","scrub",
    "sea","search","season","seat","second","secret","section","security","seed","seek",
    "segment","select","sell","seminar","senior","sense","sentence","series","service","session",
    "settle","setup","seven","shadow","shaft","shallow","share","shed","shell","sheriff",
    "shield","shift","shine","ship","shiver","shock","shoe","shoot","shop","short",
    "shoulder","shove","shrimp","shrug","shuffle","shy","sibling","sick","side","siege",
    "sight","sign","silent","silk","silly","silver","similar","simple","since","sing",
    "siren","sister","situate","six","size","skate","sketch","ski","skill","skin",
    "skirt","skull","slab","slam","sleep","slender","slice","slide","slight","slim",
    "slogan","slot","slow","slush","small","smart","smile","smoke","smooth","snack",
    "snake","snap","sniff","snow","soap","soccer","social","sock","soda","soft",
    "solar","soldier","solid","solution","solve","someone","song","soon","sorry","sort",
    "soul","sound","soup","source","south","space","spare","spatial","spawn","speak",
    "special","speed","spell","spend","sphere","spice","spider","spike","spin","spirit",
    "split","spoil","sponsor","spoon","sport","spot","spray","spread","spring","spy",
    "square","squeeze","squirrel","stable","stadium","staff","stage","stairs","stamp","stand",
    "start","state","stay","steak","steel","steep","steer","stem","step","stereo",
    "stick","still","sting","stock","stomach","stone","stool","story","stove","strategy",
    "street","strike","strong","struggle","student","stuff","stumble","style","subject","submit",
    "subway","success","such","sudden","suffer","sugar","suggest","suit","sun","sunny",
    "sunset","super","supply","support","suppose","sure","surface","surge","surprise","surround",
    "survey","suspect","sustain","swallow","swamp","swap","swarm","swear","sweet","swift",
    "swim","swing","switch","sword","symbol","symptom","syrup","system","table","tackle",
    "tag","tail","talent","talk","tank","tape","target","task","taste","tattoo",
    "taxi","teach","team","tell","ten","tenant","tennis","tent","term","test",
    "text","thank","that","theme","then","theory","there","they","thing","this",
    "thought","three","thrive","throw","thumb","thunder","ticket","tide","tiger","tilt",
    "timber","time","tiny","tip","tired","tissue","title","toast","tobacco","today",
    "toddler","toe","together","toilet","token","tomato","tomorrow","tone","tongue","tonight",
    "tool","tooth","top","topic","topple","torch","tornado","tortoise","toss","total",
    "tourist","toward","tower","town","toy","track","trade","traffic","tragic","train",
    "transfer","trap","trash","travel","tray","treat","tree","trend","trial","tribe",
    "trick","trigger","trim","trip","trophy","trouble","truck","true","truly","trumpet",
    "trust","truth","try","tube","tuition","tumble","tuna","tunnel","turkey","turn",
    "turtle","twelve","twenty","twice","twin","twist","two","type","typical","ugly",
    "umbrella","unable","unaware","uncle","uncover","under","undo","unfair","unfold","unhappy",
    "uniform","unique","unit","universe","unknown","unlock","until","unusual","unveil","update",
    "upgrade","uphold","upon","upper","upset","urban","urge","usage","use","used",
    "useful","useless","usual","utility","vacant","vacuum","vague","valid","valley","valve",
    "van","vanish","vapor","various","vast","vault","vehicle","velvet","vendor","venture",
    "venue","verb","verify","version","very","vessel","veteran","viable","vibrant","vicious",
    "victory","video","view","village","vintage","violin","virtual","virus","visa","visit",
    "visual","vital","vivid","vocal","voice","void","volcano","volume","vote","voyage",
    "wage","wagon","wait","walk","wall","walnut","want","warfare","warm","warrior",
    "wash","wasp","waste","water","wave","way","wealth","weapon","wear","weasel",
    "weather","web","wedding","weekend","weird","welcome","west","wet","whale","what",
    "wheat","wheel","when","where","whip","whisper","wide","width","wife","wild",
    "will","win","window","wine","wing","wink","winner","winter","wire","wisdom",
    "wise","wish","witness","wolf","woman","wonder","wood","wool","word","work",
    "world","worry","worth","wrap","wreck","wrestle","wrist","write","wrong","yard",
    "year","yellow","you","young","youth","zebra","zero","zone","zoo",
];

/// Convert 32 bytes of entropy to a 24-word BIP39 mnemonic phrase.
pub fn entropy_to_mnemonic(entropy: &[u8; 32]) -> Vec<String> {
    let checksum_byte = entropy[0] >> 5;
    let mut bits = [0u8; 33];
    bits[..32].copy_from_slice(entropy);
    bits[32] = checksum_byte;
    let mut words = Vec::with_capacity(24);
    for i in 0..24 {
        let bit_offset = i * 11;
        let byte_idx = bit_offset / 8;
        let bit_shift = bit_offset % 8;
        // Extract 11-bit word index starting at bit_offset
        // 256+8=264 bits, 24 words of 11 bits each
        let index = {
            let window_shift = bit_shift % 8;
            if window_shift <= 5 {
                // 11 bits span at most 2 bytes: (8 - ws) + ws + min(ws, 3) = 8 + ws >= 11
                let mut val = (bits[byte_idx] as u16) << 8;
                if byte_idx + 1 < 33 { val |= bits[byte_idx + 1] as u16; }
                (val >> (8 - window_shift)) & 0x7FF
            } else {
                // 11 bits span 3 bytes: from byte byte_idx, bit window_shift, into byte_idx+2
                let mut val = (bits[byte_idx] as u32) << 16;
                if byte_idx + 1 < 33 { val |= (bits[byte_idx + 1] as u32) << 8; }
                if byte_idx + 2 < 33 { val |= bits[byte_idx + 2] as u32; }
                // start position in 24-bit window: 23 - window_shift
                // 11 bits: positions (23-ws) down to (13-ws)
                // shift right by (13-ws) to align to LSB
                ((val >> (13 - window_shift)) & 0x7FF) as u16
            }
        };
        words.push(BIP39_WORDS[index as usize].to_string());
    }
    words
}

/// Generate a 24-word mnemonic from a wallet key's spend secret.
pub fn seed_phrase_from_key(key: &StealthKeyEntry) -> Vec<String> {
    entropy_to_mnemonic(&key.spend_secret)
}

/// Recover a spend secret from a 24-word mnemonic.
pub fn mnemonic_to_entropy(words: &[String]) -> Result<[u8; 32], String> {
    if words.len() != 24 {
        return Err("Mnemonic must be 24 words".into());
    }
    let word_map: std::collections::HashMap<&str, u16> = BIP39_WORDS.iter().enumerate()
        .map(|(i, w)| (*w, i as u16)).collect();
    let mut bit_string = Vec::with_capacity(33);
    let mut bit_buffer: u64 = 0;
    let mut bits_in_buffer = 0;
    for word_str in words {
        let idx = word_map.get(word_str.as_str())
            .ok_or_else(|| format!("Unknown BIP39 word: {}", word_str))?;
        bit_buffer = (bit_buffer << 11) | *idx as u64;
        bits_in_buffer += 11;
        while bits_in_buffer >= 8 {
            bits_in_buffer -= 8;
            bit_string.push((bit_buffer >> bits_in_buffer) as u8);
        }
    }
    if bits_in_buffer > 0 {
        bit_string.push((bit_buffer << (8 - bits_in_buffer)) as u8);
    }
    if bit_string.len() < 33 {
        return Err("Mnemonic decoding failed".into());
    }
    let mut entropy = [0u8; 32];
    entropy.copy_from_slice(&bit_string[..32]);
    let expected_checksum = entropy[0] >> 5;
    let actual_checksum = bit_string[32] >> 5;
    if expected_checksum != actual_checksum {
        return Err("Mnemonic checksum mismatch".into());
    }
    Ok(entropy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use curve25519_dalek::traits::Identity;

    #[test]
    fn test_wallet_keygen() {
        let mut w = Wallet { keys: vec![] };
        w.new_key("test");
        assert_eq!(w.keys.len(), 1);
        assert!(w.keys[0].address().len() == 64);
        // legacy key should be generated (P1-3)
        assert_eq!(w.keys[0].legacy_public_key.len(), 32);
    }

    #[test]
    fn test_wallet_scan_empty() {
        let w = Wallet { keys: vec![] };
        let utxo_set = UtxoSet::new();
        let owned = w.scan_utxos(&utxo_set);
        assert!(owned.is_empty());
    }

    #[test]
    fn test_create_tx_zero_amount_rejected() {
        let mut w = Wallet { keys: vec![] };
        w.new_key("test");
        let utxo_set = UtxoSet::new();
        let dummy_addr = crate::privacy::StealthAddress {
            spend_key: curve25519_dalek::ristretto::CompressedRistretto([0u8; 32]).decompress().unwrap_or(crate::privacy::ring_g()),
            view_key: curve25519_dalek::ristretto::RistrettoPoint::identity(),
        };
        let result = create_private_tx(&w, &dummy_addr, 0, &utxo_set, &mut rand::thread_rng());
        assert!(result.is_err(), "zero amount tx should be rejected");
    }

    /// Create a properly-formed stealth UTXO owned by `recipient`, with
    /// correct one-time address derivation, ephemeral key, commitment, and range proof.
    fn seed_stealth_utxo(
        utxo_set: &mut UtxoSet,
        recipient: &StealthAddress,
        amount: u64,
        rng: &mut ThreadRng,
    ) {
        use crate::privacy::{Commitment, RangeProof};

        // Derive one-time destination from recipient's stealth address
        let (dest, _r_ephem) = recipient.derive_destination(rng);

        // Create commitment + range proof
        let (range_proof, blinding) = RangeProof::prove_with_blinding(amount, 32, rng);
        let comm = Commitment::new_with_blinding(amount, blinding);

        let mut tx_hash = [0u8; 32];
        rng.fill_bytes(&mut tx_hash);

        utxo_set.add_transaction_outputs(&tx_hash, &Transaction {
            version: 1,
            inputs: vec![],
            outputs: vec![TxOutput {
                amount,
                pubkey_hash: [0u8; 20],
                spendable_after: 0,
                stealth_dest: Some(dest.dest.compress().to_bytes()),
                commitment_bytes: Some(comm.0.compress().to_bytes()),
                range_proof_bytes: Some(serde_json::to_vec(&range_proof).unwrap()),
                ephemeral: Some(dest.ephemeral.compress().to_bytes()),
            }],
            ring_size: 1,
            signatures: vec![],
            mlsag: None,
            ring_members: None,
        }, 0, 0);
    }

    #[test]
    fn test_create_private_tx_roundtrip() {
        let mut rng = rand::thread_rng();

        // Create Alice and Bob wallets
        let mut alice_w = Wallet { keys: vec![] };
        alice_w.new_key("alice");
        let mut bob_w = Wallet { keys: vec![] };
        bob_w.new_key("bob");

        let alice_addr = alice_w.keys[0].stealth_address().unwrap();
        let bob_addr = bob_w.keys[0].stealth_address().unwrap();

        // Seed 11 stealth UTXOs: 1 spendable by Alice, 10 decoys
        let mut utxo_set = UtxoSet::new();
        seed_stealth_utxo(&mut utxo_set, &alice_addr, 500, &mut rng);
        for _ in 0..10 {
            let (dummy_addr, _) = StealthAddress::generate(&mut rng);
            seed_stealth_utxo(&mut utxo_set, &dummy_addr, 100, &mut rng);
        }

        // Verify Alice can see her UTXO before spend
        let alice_owned = alice_w.scan_utxos(&utxo_set);
        assert_eq!(alice_owned.len(), 1, "Alice should see 1 UTXO");
        assert_eq!(alice_owned[0].commitment_val, 500);

        // Alice creates private tx to Bob
        let tx = create_private_tx(&alice_w, &bob_addr, 100, &utxo_set, &mut rng)
            .expect("Private tx creation");

        // State validates: MLSAG + range proofs + Pedersen balance
        utxo_set.spend_transaction_inputs(&tx, 1)
            .expect("State accepts private tx");

        // Add outputs to UTXO set (spend_transaction_inputs only removes inputs)
        let tx_hash = tx.hash();
        utxo_set.add_transaction_outputs(&tx_hash, &tx, 1, 0);

        // Bob scans and finds his UTXO
        let bob_owned = bob_w.scan_utxos(&utxo_set);
        let bob_balance: u64 = bob_owned.iter().map(|o| o.entry.amount).sum();
        assert_eq!(bob_balance, 100, "Bob receives 100");

        // Alice's change (400) should also be findable
        let alice_after = alice_w.scan_utxos(&utxo_set);
        let alice_balance: u64 = alice_after.iter().map(|o| o.entry.amount).sum();
        assert_eq!(alice_balance, 400, "Alice keeps 400 in change");
    }
}
