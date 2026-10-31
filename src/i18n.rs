//! Localization: English, Portuguese and Turkish string tables.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Lang {
    En,
    Pt,
    #[default]
    Tr,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::En, Lang::Pt, Lang::Tr];

    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Pt => "Português",
            Lang::Tr => "Türkçe",
        }
    }

    fn index(self) -> usize {
        match self {
            Lang::En => 0,
            Lang::Pt => 1,
            Lang::Tr => 2,
        }
    }
}

/// Defines both `KEYS` and the string table from one list so they cannot diverge.
macro_rules! strings {
    ($( $key:literal => [$en:literal, $pt:literal, $tr:literal] ),* $(,)?) => {
        /// Every translation key, in table order.
        pub const KEYS: &[&str] = &[ $( $key ),* ];

        static TABLE: &[(&str, [&str; 3])] = &[ $( ($key, [$en, $pt, $tr]) ),* ];
    };
}

strings! {
    "clear" => ["Clear", "Limpar", "Temizle"],
    "copy" => ["Copy", "Copiar", "Kopyala"],
    "countall" => ["Quantity of entries:", "Quantidade de entradas:", "Girdi sayısı:"],
    "countdirectories" => ["Directories:", "Diretórios:", "Dizinler:"],
    "countfiles" => ["Files:", "Arquivos:", "Dosyalar:"],
    "countfirst" => ["Oldest entry:", "Entrada mais antiga:", "En eski girdi:"],
    "currentfile" => ["Current file", "Arquivo atual", "Geçerli dosya"],
    "datagrid" => ["Data Grid", "Grade de dados", "Veri Tablosu"],
    "date" => ["Date", "Data", "Tarih"],
    "directory" => ["Directory", "Diretório", "Dizin"],
    "directorytree" => ["Directory Tree", "Arvore de diretórios", "Dizin Ağacı"],
    "drive" => ["Drive", "Disco", "Sürücü"],
    "enterdir" => ["Enter entry directory", "Entrar no diretório da entrada", "Dosya konumunu aç"],
    "entryinfo" => ["Show file information", "Mostrar informações do arquivo", "Dosya bilgilerini göster"],
    "export" => ["Export", "Exportar", "Dışa aktar"],
    "file" => ["File", "Arquivo", "Dosya"],
    "filedeleted" => ["File was deleted", "Arquivo foi deletado", "Dosya silindi"],
    "fileid" => ["File Identification", "Identificador do arquivo", "Dosya Kimliği"],
    "format" => ["Format", "Formato", "Biçim"],
    "formdrive" => ["Drive selection", "Seleção de disco", "Sürücü seçimi"],
    "formentryinfo" => ["Entry information", "Informações da entrada", "Girdi bilgileri"],
    "info" => ["Information", "Informação", "Bilgi"],
    "language" => ["Language", "Lingua", "Dil"],
    "lbUsnJrnl" => ["last modification $UsnJrnl:$J", "última modificação $UsnJrnl:$J", "son değişiklik $UsnJrnl:$J"],
    "lordyzagatdesc" => ["Idea for the project, translated reasons", "Ideia para o projeto, traduziu razões", "Proje fikri, çevrilmiş nedenler"],
    "name" => ["Name", "Nome", "Ad"],
    "nolandesc" => ["OpenByFileId Win32 API wrapper for C#", "OpenByFileId Win32 API compatibilização para C#", "C# için OpenByFileId Win32 API sarmalayıcısı"],
    "poneidesc" => ["Main developer", "Desenvolvedor principal", "Ana geliştirici"],
    "reason" => ["Reason", "Razão", "Neden"],
    "root" => ["Root", "Raiz", "Kök"],
    "scan" => ["Scan", "Escanear", "Tara"],
    "search" => ["Search", "Pesquisar", "Ara"],
    "select" => ["Select", "Selecionar", "Seç"],
    "skipperdesc" => ["NTFS UsnJournal Win32 API wrapper for C#", "NTFS UsnJournal Win32 API compatibilização para C#", "C# için NTFS UsnJournal Win32 API sarmalayıcısı"],
    "statusdesc-1" => [
        "The program couldn't create a handle to the selected drive, to query the current journal state.",
        "O programa não conseguiu manejar o disco selecionado, para consultar o estado atual do journal.",
        "Program, günlüğün geçerli durumunu sorgulamak için seçilen sürücüye bir tanıtıcı (handle) oluşturamadı."
    ],
    "statusdesc-2" => [
        "The program couldn't query the current state of the journal. Make sure the selected drive has journaling enabled.",
        "O programa não conseguiu consultar o estado atual do journal. Certifique-se de que o disco selecionado tenha o journal habilitado.",
        "Program günlüğün geçerli durumunu sorgulayamadı. Seçilen sürücüde günlüklemenin etkin olduğundan emin olun."
    ],
    "statusdesc-3" => [
        "The program couldn't call DeviceIoControl properly to return all the journal entries in the drive.",
        "O programa não conseguiu chamar o DeviceIoControl corretamente para retornar todos as entradas do journal do disco.",
        "Program, sürücüdeki tüm günlük girdilerini döndürmek için DeviceIoControl'ü doğru şekilde çağıramadı."
    ],
    "statusdesc-4" => [
        "The program couldn't resolve file ID's properly. Is something blocking the program?",
        "O programa não conseguiu resolver as identificações dos arquivos corretamente. Algo está bloqueando o programa?",
        "Program dosya kimliklerini doğru şekilde çözümleyemedi. Programı engelleyen bir şey mi var?"
    ],
    "statusdesc-5" => [
        "The program couldn't parse journal entries correctly. This could be the program's fault.",
        "O programa não conseguiu analisar corretamente as entradas do journal corretamente. Isto pode ser culpa do programa.",
        "Program günlük girdilerini doğru şekilde ayrıştıramadı. Bu programdan kaynaklanıyor olabilir."
    ],
    "statusdesc0" => [
        "Please select the desired drive and start filtering journal entries.",
        "Por favor, selecione o disco desejado e incie a filtragem do journal.",
        "Lütfen istediğiniz sürücüyü seçin ve günlük girdilerini filtrelemeye başlayın."
    ],
    "statusdesc1" => [
        "To query the journal, we have to make sure we can create handles to it without problems.",
        "Para consultar o journal, temos que ter certeza de que podemos criar handles sem problemas.",
        "Günlüğü sorgulamak için ona sorunsuz tanıtıcılar oluşturabildiğimizden emin olmamız gerekir."
    ],
    "statusdesc2" => [
        "We need to query the current state of the journal to get it's values.",
        "Precisamos consultar o estado atual do journal para obter seus valores.",
        "Değerlerini almak için günlüğün geçerli durumunu sorgulamamız gerekiyor."
    ],
    "statusdesc3" => [
        "Calling DeviceIoControl to give us all the entries in the current journal ID.",
        "Chamando de DeviceIoControl para nos dar todas as entradas do ID do journal atual.",
        "Geçerli günlük kimliğindeki tüm girdileri almak için DeviceIoControl çağrılıyor."
    ],
    "statusdesc4" => [
        "File ID's aren't human readable paths, so we need to resolve all the possible ones.",
        "Os ID's dos arquivos não são caminhos legíveis por humanos, então precisamos resolver todos os possíveis.",
        "Dosya kimlikleri insanların okuyabileceği yollar değildir, bu yüzden olası tümünü çözümlememiz gerekir."
    ],
    "statusdesc5" => [
        "Parsing all journal entries into classes that we will modify and query later.",
        "Transformando todas as entradas de journal em classes que modificaremos e consultaremos mais tarde.",
        "Tüm günlük girdileri, daha sonra değiştirip sorgulayacağımız sınıflara ayrıştırılıyor."
    ],
    "statusdesc6" => [
        "Select a layout type to view the collected data. Each will visualize data on a different way.",
        "Selecione um tipo de layout para visualizar os dados coletados. Cada um mostrará os dados de uma maneira diferente.",
        "Toplanan verileri görüntülemek için bir düzen türü seçin. Her biri verileri farklı şekilde gösterir."
    ],
    "statusgrid0" => ["Parsing journal entries to rows...", "Transformando entradas em fileiras...", "Günlük girdileri satırlara ayrıştırılıyor..."],
    "statusgrid1" => ["Adding rows to control...", "Adicionando fileiras à grade...", "Satırlar tabloya ekleniyor..."],
    "statustitle-1" => ["Couldn't get handle to volume.", "Não foi possível manejar o disco selecionado.", "Birim tanıtıcısı alınamadı."],
    "statustitle-2" => ["Couldn't get current journal state.", "Não foi possível retornar o estado atual do journal.", "Geçerli günlük durumu alınamadı."],
    "statustitle-3" => ["Couldn't query journal entries.", "Não foi possível consultar as entradas do journal.", "Günlük girdileri sorgulanamadı."],
    "statustitle-4" => ["Couldn't resolve ID's.", "Não foi possível resolver os ID's.", "Kimlikler çözümlenemedi."],
    "statustitle-5" => ["Couldn't parse entries.", "Não foi possível analisar as entradas.", "Girdiler ayrıştırılamadı."],
    "statustitle0" => ["Waiting input...", "Aguardando entrada...", "Girdi bekleniyor..."],
    "statustitle1" => ["Getting handle to volume...", "Getting handle to volume...", "Birim tanıtıcısı alınıyor..."],
    "statustitle2" => ["Getting current journal state...", "Obtendo o estado atual do journal...", "Geçerli günlük durumu alınıyor..."],
    "statustitle3" => ["Querying journal entries...", "Consultando as entradas do journal...", "Günlük girdileri sorgulanıyor..."],
    "statustitle4" => ["Resolving file ID's...", "Resolvendo identificações de arquivos...", "Dosya kimlikleri çözümleniyor..."],
    "statustitle5" => ["Parsing entries...", "Analisando entradas...", "Girdiler ayrıştırılıyor..."],
    "statustitle6" => ["All done!", "Tudo pronto!", "Hepsi tamam!"],
    "totalfreespace" => ["Total free space", "Espaço total livre", "Toplam boş alan"],
    "totalsize" => ["Total size", "Espaço total", "Toplam boyut"],
    "type" => ["Type", "Tipo", "Tür"],
    // ---- new keys ----
    "layout" => ["Layout", "Layout", "Düzen"],
    "deleted" => ["Deleted", "Deletado", "Silindi"],
    "renamednew" => ["Renamed: new", "Renomeado: novo", "Yeniden adlandırıldı: yeni"],
    "renamedold" => ["Renamed: old", "Renomeado: antigo", "Yeniden adlandırıldı: eski"],
    "streamchange" => ["Stream change", "Alteração de fluxo", "Akış değişikliği"],
    "basicinfochange" => ["Basic info change", "Alteração de informações básicas", "Temel bilgi değişikliği"],
    "allcolumns" => ["All columns", "Todas as colunas", "Tüm sütunlar"],
    "shown" => ["Shown:", "Exibidas:", "Gösterilen:"],
    "exportcsv" => ["Export to CSV...", "Exportar para CSV...", "CSV olarak dışa aktar..."],
    "exportdone" => ["Exported rows:", "Linhas exportadas:", "Dışa aktarılan satırlar:"],
    "exportfailed" => ["Export failed:", "Falha ao exportar:", "Dışa aktarma başarısız:"],
    "selecteddrive" => ["Selected drive:", "Disco selecionado:", "Seçilen sürücü:"],
    "nodrive" => ["No drive selected", "Nenhum disco selecionado", "Sürücü seçilmedi"],
    "deleteddir" => ["(deleted)", "(deletado)", "(silindi)"],
    "live" => ["Live monitoring", "Monitoramento ao vivo", "Canlı izleme"],
    "liveon" => ["Live", "Ao vivo", "Canlı"],
    "livestopped" => ["Live monitoring stopped:", "Monitoramento ao vivo parado:", "Canlı izleme durdu:"],
    "copyrows" => ["Copy selected rows", "Copiar linhas selecionadas", "Seçili satırları kopyala"],
    "exportselected" => ["Export selected rows...", "Exportar linhas selecionadas...", "Seçili satırları dışa aktar..."],
    "selected" => ["Selected:", "Selecionadas:", "Seçili:"],
    "alldrives" => ["All drives", "Todos os discos", "Tüm sürücüler"],
    "scanall" => ["Scan all drives", "Escanear todos os discos", "Tüm sürücüleri tara"],
    "skipped" => ["Skipped:", "Ignorado:", "Atlandı:"],
    "exportas" => ["Export (CSV / JSON / HTML)...", "Exportar (CSV / JSON / HTML)...", "Dışa aktar (CSV / JSON / HTML)..."],
    "namehistory" => ["Name history:", "Histórico de nomes:", "Ad geçmişi:"],
    "lifetime" => ["Lifetime:", "Tempo de vida:", "Yaşam süresi:"],
    "createdat" => ["Created:", "Criado:", "Oluşturuldu:"],
    "deletedat" => ["Deleted:", "Excluído:", "Silindi:"],
    "shortlived" => ["Short-lived (< 1 min)", "Vida curta (< 1 min)", "Kısa ömürlü (< 1 dk)"],
    "timeline" => ["Timeline", "Linha do tempo", "Zaman çizelgesi"],
    "timefilter" => ["Time:", "Hora:", "Zaman:"],
    "created" => ["Created", "Criados", "Oluşturulan"],
    "renamed" => ["Renamed", "Renomeados", "Yeniden adlandırılan"],
    "generated" => ["Generated:", "Gerado:", "Oluşturulma:"],
    "filterhint" => ["Filter...", "Filtrar...", "Filtrele..."],
    "duvodesc" => ["Turkish translation, Rust port", "Tradução para turco, port para Rust", "Türkçe çeviri, Rust çevirisi"],
    "journalid" => ["Journal ID:", "ID do journal:", "Günlük kimliği:"],
    "maxsize" => ["Maximum size:", "Tamanho máximo:", "Maksimum boyut:"],
    "close" => ["Close", "Fechar", "Kapat"],
    "version" => ["Version", "Versão", "Sürüm"],
    "openinexplorer" => ["Open in Explorer", "Abrir no Explorer", "Gezgin'de aç"],
    "reasonfilters" => ["Reason filters:", "Filtros de razão:", "Neden filtreleri:"],
    "notadmin" => [
        "JournalTrace must be run as administrator to read the USN journal.",
        "O JournalTrace deve ser executado como administrador para ler o journal USN.",
        "USN günlüğünü okumak için JournalTrace yönetici olarak çalıştırılmalıdır."
    ],
    "scanning" => ["Scanning...", "Escaneando...", "Taranıyor..."],
    "entries" => ["entries", "entradas", "girdi"],
    "nodata" => ["No entries in this directory.", "Nenhuma entrada neste diretório.", "Bu dizinde girdi yok."],
}

/// Translated text for `key`. Falls back to English, then to the key itself.
pub fn t(lang: Lang, key: &'static str) -> &'static str {
    for (k, vals) in TABLE {
        if *k == key {
            let v = vals[lang.index()];
            return if v.is_empty() { vals[0] } else { v };
        }
    }
    key
}

static REASONS_EN: [&str; 24] = [
    "Data overwrite",
    "Data extend",
    "Data truncation",
    "Named data overwrite",
    "Named data extend",
    "Named data truncation",
    "File create",
    "File delete",
    "EA Change",
    "Security change",
    "Rename: old name",
    "Rename: new name",
    "Indexable change",
    "Basic info change",
    "Hard link change",
    "Compression change",
    "Encryption change",
    "Object ID change",
    "Reparse point change",
    "Stream change",
    "Transacted change",
    "Integrity change",
    "Desired storage class change",
    "Close",
];

static REASONS_PT: [&str; 24] = [
    "Substituição de dados",
    "Extensão de dados",
    "Truncagem de dados",
    "Substituição de dados nomeados",
    "Extensão de dados nomeados",
    "Truncagem de dados nomeados",
    "Criação de arquivo",
    "Exclusão de arquivo",
    "Alteração de EA",
    "Alteração de segurança",
    "Renomear: nome antigo",
    "Renomear: novo nome",
    "Alteração de indexável",
    "Alteração de informações básicas",
    "Alteração de link físico",
    "Alteração de compressão",
    "Alteração de criptografia",
    "Alteração de ID de objeto",
    "Alteração do ponto de nova análise",
    "Alteração de fluxo",
    "Alteração transacionada",
    "Alteração de integridade",
    "Alteração da classe de armazenamento desejada",
    "Fechar",
];

static REASONS_TR: [&str; 24] = [
    "Veri üzerine yazma",
    "Veri genişletme",
    "Veri kesme",
    "Adlandırılmış veri üzerine yazma",
    "Adlandırılmış veri genişletme",
    "Adlandırılmış veri kesme",
    "Dosya oluşturma",
    "Dosya silme",
    "EA değişikliği",
    "Güvenlik değişikliği",
    "Yeniden adlandırma: eski ad",
    "Yeniden adlandırma: yeni ad",
    "Dizinlenebilirlik değişikliği",
    "Temel bilgi değişikliği",
    "Sabit bağlantı değişikliği",
    "Sıkıştırma değişikliği",
    "Şifreleme değişikliği",
    "Nesne kimliği değişikliği",
    "Yeniden ayrıştırma noktası değişikliği",
    "Akış değişikliği",
    "İşlemsel değişiklik",
    "Bütünlük değişikliği",
    "İstenen depolama sınıfı değişikliği",
    "Kapat",
];

/// Names of the 24 USN reason flags (bit order, with CLOSE last).
pub fn reason_names(lang: Lang) -> &'static [&'static str; 24] {
    match lang {
        Lang::En => &REASONS_EN,
        Lang::Pt => &REASONS_PT,
        Lang::Tr => &REASONS_TR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_match_table_and_are_unique() {
        assert_eq!(KEYS.len(), TABLE.len());
        for (i, k) in KEYS.iter().enumerate() {
            assert_eq!(*k, TABLE[i].0);
            assert_eq!(KEYS.iter().filter(|x| *x == k).count(), 1, "duplicate {k}");
        }
    }

    #[test]
    fn every_key_present_in_every_language() {
        for &key in KEYS {
            for lang in Lang::ALL {
                let v = t(lang, key);
                assert!(!v.is_empty(), "{key} empty for {lang:?}");
            }
        }
    }

    #[test]
    fn spot_checks() {
        assert_eq!(t(Lang::En, "clear"), "Clear");
        assert_eq!(t(Lang::Pt, "clear"), "Limpar");
        assert_eq!(t(Lang::Tr, "clear"), "Temizle");
        assert_eq!(t(Lang::En, "lbUsnJrnl"), "last modification $UsnJrnl:$J");
        assert_eq!(t(Lang::Pt, "statustitle1"), "Getting handle to volume...");
        assert_ne!(t(Lang::Pt, "close"), t(Lang::En, "close"));
        assert_ne!(t(Lang::Tr, "close"), t(Lang::En, "close"));
    }

    #[test]
    fn unknown_key_falls_back_to_key() {
        for lang in Lang::ALL {
            assert_eq!(t(lang, "no-such-key"), "no-such-key");
        }
    }

    #[test]
    fn reason_names_complete() {
        for lang in Lang::ALL {
            let r = reason_names(lang);
            assert_eq!(r.len(), 24);
            assert!(r.iter().all(|s| !s.is_empty()));
        }
        assert_eq!(reason_names(Lang::En)[6], "File create");
        assert_eq!(reason_names(Lang::En)[7], "File delete");
        assert_eq!(reason_names(Lang::En)[23], "Close");
        assert_eq!(reason_names(Lang::Pt)[19], "Alteração de fluxo");
        assert_eq!(reason_names(Lang::Pt)[23], "Fechar");
        assert_eq!(reason_names(Lang::Tr)[23], "Kapat");
    }

    #[test]
    fn lang_basics() {
        assert_eq!(Lang::default(), Lang::Tr);
        assert_eq!(Lang::En.native_name(), "English");
        assert_eq!(Lang::Pt.native_name(), "Português");
        assert_eq!(Lang::Tr.native_name(), "Türkçe");
    }
}
