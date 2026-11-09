// SPDX-License-Identifier: GPL-3.0-or-later
//! Ordem de construção: fecho das dependências + ordenação topológica.

use crate::receita::{resolver_dep, Receita};
use std::collections::{BTreeMap, BTreeSet};

/// Devolve a lista de pacotes a construir, em ordem, para obter `alvos`
/// na arquitetura `arq`. Dependências de construção e de execução contam.
pub fn plano(
    alvos: &[String],
    todas: &BTreeMap<String, Receita>,
    arq: &str,
) -> Result<Vec<String>, String> {
    #[derive(Clone, Copy, PartialEq)]
    enum Marca {
        Visitando,
        Feito,
    }
    let mut marcas: BTreeMap<String, Marca> = BTreeMap::new();
    let mut ordem = Vec::new();

    fn visitar(
        nome: &str,
        todas: &BTreeMap<String, Receita>,
        arq: &str,
        marcas: &mut BTreeMap<String, Marca>,
        ordem: &mut Vec<String>,
        pilha: &mut Vec<String>,
    ) -> Result<(), String> {
        match marcas.get(nome) {
            Some(Marca::Feito) => return Ok(()),
            Some(Marca::Visitando) => {
                pilha.push(nome.to_string());
                let inicio = pilha.iter().position(|n| n == nome).unwrap_or(0);
                return Err(format!("dependência circular: {}", pilha[inicio..].join(" → ")));
            }
            None => {}
        }
        let r = todas.get(nome).ok_or_else(|| format!("pacote '{nome}' não existe"))?;
        if !r.suporta(arq) {
            let via = pilha.last().map(|p| format!(" (exigido por '{p}')")).unwrap_or_default();
            return Err(format!("'{nome}' não suporta {arq}{via}"));
        }
        marcas.insert(nome.to_string(), Marca::Visitando);
        pilha.push(nome.to_string());
        // Ordem determinística: dependências em ordem alfabética dentro de cada lista.
        let mut deps: Vec<String> = Vec::new();
        for d in r.dep_construcao.iter().chain(&r.dep_execucao) {
            for x in resolver_dep(d, todas) {
                if x != nome && !deps.contains(&x) {
                    deps.push(x);
                }
            }
        }
        for d in deps {
            visitar(&d, todas, arq, marcas, ordem, pilha)?;
        }
        pilha.pop();
        marcas.insert(nome.to_string(), Marca::Feito);
        ordem.push(nome.to_string());
        Ok(())
    }

    let alvos: BTreeSet<String> = if alvos.is_empty() {
        todas.values().filter(|r| r.suporta(arq)).map(|r| r.nome.clone()).collect()
    } else {
        let mut s = BTreeSet::new();
        for a in alvos {
            let r = resolver_dep(a, todas);
            if r.is_empty() {
                return Err(format!("'{a}' não é pacote nem categoria"));
            }
            s.extend(r);
        }
        s
    };
    // A ordem final é ditada pelas dependências; entre pacotes independentes,
    // vale a ordem alfabética (determinística, para builds reproduzíveis).
    for a in &alvos {
        visitar(a, todas, arq, &mut marcas, &mut ordem, &mut Vec::new())?;
    }
    Ok(ordem)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::receita::de_texto;
    use std::path::Path;

    fn r(nome: &str, cat: &str, deps: &[&str], arqs: &[&str]) -> Receita {
        let deps = deps.iter().map(|d| format!("\"{d}\"")).collect::<Vec<_>>().join(",");
        let arqs = arqs.iter().map(|d| format!("\"{d}\"")).collect::<Vec<_>>().join(",");
        let txt = format!(
            "[pacote]\nnome=\"{nome}\"\nversao=\"1\"\nresumo=\"r\"\nlicenca=\"MIT\"\ncategoria=\"{cat}\"\n\
             [dependencias]\nconstrucao=[{deps}]\n[construcao]\npassos=[\"true\"]\n\
             [arquiteturas]\nsuportadas=[{arqs}]\n"
        );
        de_texto(&txt, Path::new(&format!("{nome}/receita.toml"))).unwrap()
    }

    fn mapa(rs: Vec<Receita>) -> BTreeMap<String, Receita> {
        rs.into_iter().map(|r| (r.nome.clone(), r)).collect()
    }

    #[test]
    fn ordena_dependencias() {
        let t = mapa(vec![
            r("gcc", "toolchain", &["binutils"], &[]),
            r("binutils", "toolchain", &[], &[]),
            r("zlib", "base", &["toolchain"], &[]),
        ]);
        let p = plano(&["zlib".into()], &t, "x86_64").unwrap();
        assert_eq!(p, vec!["binutils", "gcc", "zlib"]);
    }

    #[test]
    fn detecta_ciclo() {
        let t = mapa(vec![r("a", "x", &["b"], &[]), r("b", "x", &["a"], &[])]);
        let e = plano(&["a".into()], &t, "x86_64").unwrap_err();
        assert!(e.contains("circular"), "{e}");
    }

    #[test]
    fn respeita_arquitetura() {
        let t = mapa(vec![r("a", "x", &["b"], &[]), r("b", "x", &[], &["x86_64"])]);
        let e = plano(&["a".into()], &t, "aarch64").unwrap_err();
        assert!(e.contains("não suporta aarch64"), "{e}");
    }
}
