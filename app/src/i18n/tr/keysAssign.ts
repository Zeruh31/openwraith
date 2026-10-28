// Tuşlar > Atamalar: sekmenin kendi metinleri (katman seçimiyle klavye, seçili
// tuş, takas, boş yuvaya başvuran tuşlar, uygulama çubuğu). Ortak Tuşlar
// sözcükleri (katmanlar, eylemler, yerleşik işlevler, inceleme) keys.ts içinde.

import type { keysAssign as en } from "../en/keysAssign";
import type { Messages } from "../types";

export const keysAssign: Messages<typeof en> = {
  title: "Atamalar",
  intro:
    "Bir tuşun temel ve Fn katmanında ne yaptığını görmek ve değiştirmek için onu seçin. Değişiklikleri inceleyip Klavyeye yaz düğmesine basmadan hiçbir şey yazılmaz.",
  layer: {
    label: "Katman",
    hint: "Temel ve Fn katmanı arasında geçmek için L tuşuna basın.",
    shown: {
      base: "Temel katman gösteriliyor",
      fn: "Fn katmanı gösteriliyor",
    },
  },
  board: {
    title: "Tuşların yaptıkları",
    label: {
      base: "Temel katmandaki tuşlar",
      fn: "Fn katmanındaki tuşlar",
    },
    hint: "Bir tuşu seçmek için tıklayın ya da ok tuşlarıyla üzerine gelip Boşluk'a basın. Enter ya da çift tıklama Değiştir… penceresini açar.",
  },
  legend: {
    label: "Tuşlardaki işaretlerin anlamı",
    remapped: "Değiştirildi: tuşun şimdi yaptığı iş, altında kendi yazısı",
    builtin: "Klavyenin kendi Fn işlevi",
    pending: "Henüz yazılmadı",
    broken: "Boş bir makroya ya da gelişmiş tuşa başvuruyor",
  },
  cap: {
    macro: (p) => `M${p.n}`,
    advanced: (p) => `G${p.n}`,
    advancedTypes: {
      dks: "DKS",
      mod_tap: "MT",
      toggle: "TGL",
    },
  },
  keyInfo: {
    now: (p) => `Şu an: ${p.what}.`,
    draft: (p) => `Henüz yazılmadı: ${p.next}.`,
    broken: "Boş bir yuvaya başvuruyor, bu yüzden hiçbir şey yapmıyor.",
    unverified: "Henüz bir W75'te doğrulanmadı.",
  },
  notAssignable: {
    fnLocked: "Bu tuşun Fn katmanındaki girdisi değiştirilemez: klavye onu Fn tuşu için ayırır.",
  },
  knob: {
    entriesTitle: (p) => `Kayıtlı girdiler (${p.layer})`,
    bytes: (p) => `Döner düğme girdisi ${p.n}: ${p.hex}`,
  },
  summary: {
    title: "Seçili tuş",
    none: "Ne yaptığını görmek ve değiştirmek için klavyeden bir tuş seçin.",
    does: "Ne yapar",
    origin: "Tür",
    slot: "Yuva içeriği",
    emptySlot: "Boş: yuva doldurulana ya da tuş sıfırlanana dek tuş hiçbir şey yapmaz.",
    builtin: "Klavyenin kendi işlevi",
    otherLayer: {
      base: "Temel katmanda",
      fn: "Fn katmanında",
    },
    after: "Yazıldıktan sonra",
    openMacros: "Makrolar sekmesini aç",
    openAdvanced: "Gelişmiş tuşlar sekmesini aç",
    bytes: (p) => `${p.layer}: ${p.hex}`,
    keptText: (p) => `Okunan: ${p.text}`,
    builtinReplaced: (p) =>
      `${p.key} bu profilde şu an klavyenin kendi işlevini (${p.function}) çalıştırmıyor. Varsayılana döndürmek onu geri getirir.`,
  },
  actions: {
    label: "Seçili tuşu değiştir",
    change: "Değiştir…",
    reset: "Varsayılana döndür",
    swap: "Takas et…",
    keep: "Klavyedekini koru",
    reasons: {
      alreadyDefault: "Tuş zaten varsayılan işini yapıyor.",
      keptAsRead:
        "Bu girdi okunduğu gibi korunur, bu yüzden taşınamaz. Yine de değiştirebilir ya da varsayılana döndürebilirsiniz.",
      confirmFirst: "Bu tuşu değiştirmek için yukarıdaki onay kutusunu işaretleyin.",
    },
  },
  picker: {
    criticalConfirmed:
      "Bu kısayolun yerine başka bir şey atanmasını onayladınız. Değişiklik ancak siz onu yazdığınızda uygulanır.",
  },
  swap: {
    title: (p) => `Takas: ${p.key} ↔ …`,
    intro: "İki tuş bu katmanda yaptıkları işi birbiriyle değiştirir. İnceleyip yazmadan hiçbir şey yazılmaz.",
    label: "Diğer tuş",
    result: (p) => `${p.a} şunu yapacak: ${p.aNew}. ${p.b} şunu yapacak: ${p.bNew}.`,
    confirm: "Takas et",
    pickKey: "Diğer tuşu seçin.",
    reasons: {
      kept: "Okunduğu gibi korunur: taşınamaz",
      same: "Zaten aynı işi yapıyor",
      critical: "Önce bu tuşu seçip kısayolunun değiştirilmesini onaylayın",
    },
  },
  broken: {
    title: "Boş bir yuvaya başvuran tuşlar",
    intro:
      "Bu tuşlar içi boş bir makroya ya da gelişmiş tuşa başvuruyor, bu yüzden hiçbir şey yapmıyor. Onları burada sıfırlayın ya da yuvayı Makrolar veya Gelişmiş tuşlar sekmesinde doldurun.",
    emptySlot: (p) => `${p.key}, boş olan ${p.slot} yuvasına başvuruyor.`,
    malformed: (p) => `${p.key}, OpenWraith'in okuyamadığı bir başvuru içeriyor.`,
    reset: "Tuşu sıfırla",
    resetLabel: (p) => `${p.key} tuşunu sıfırla`,
    inDraft: "Taslakta sıfırlandı (henüz yazılmadı).",
    notResettable: "OpenWraith bu tuşu burada sıfırlayamaz.",
  },
  apply: {
    idle: "Yazılmamış tuş değişikliği yok.",
    notWritten: "Henüz yazılmadı",
    count: (p) => `${p.n} tuş değişecek`,
  },
  draftDiscarded: (p) => `Yazılmamış tuş değişiklikleri atıldı: klavye artık ${p.n} numaralı profilde.`,
  draftDiscardedReconnect: "Yazılmamış tuş değişiklikleri atıldı: klavye yeniden bağlandı.",
};
