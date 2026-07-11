# Architecture des fichiers : 

---

==Et c'est en regardant que je vois qu'on a pris de larges libertés sur mon archi de base. Donc on retourne à ma base qui a été étudiée et pas sortie *ex nihilo* de mon cul.==
	
---	


	
	## 1_atelier : 
	
==Espace de travail actif. `liens/` est la table de liens (symlinks vers les fichiers en cours, ex-06_session) ; `sticky/` les sticky (No shit, Sherlock). Mais vu que c'est mon atelier, j'y fous ce que je veux, OK ? Même des photos de Coco lovelock si ça me chante, compris ?==
	
		1_atelier/sticky,
		1_atelier/liens,
		1_Scène en cours d'écriture
		~ Symlinks
		- Mes md bordel 
		
    ## 2_todo, 
    
    Fichier To Do à étudier pour être relié au SQL par sticky mais aussi me laisser diviser en plusieurs fichiers au besoin, donc 1.md -> sticky et les autres des To Do, à vous de me faire un truc AuDHD friendly XD.
    
    ## Plan narratif etc.
    
		3_plan/chapitrages,
		3_plan/chronologie,
		3_plan/révélations,
		3_plan/systèmes,
		3_plan/archives_plan,
    
    ## 4_fiches >> à templétiser au moins au niveau du YAML supérieur  
    
		4_fiches/personnages,
		4_fiches/lieux,
		4_fiches/objets,
		4_fiches/concepts,
		4_fiches/entités,
		4_fiches/organisations,
		4_fiches/documents_internes,
		4_fiches/fragments_lore,
    
    ## 1_Scène
    
Alors comme ça n'a pas été saisi, je ne demande pas une validation : c'est comme ça et on ne discute pas. Toutes les scènes « files by files », nomenclature « C.s » -> Nbr chapitre.num de scène. Donc classement en 1.1, 1.2, 1.3 + des interludes. Chaque scène a un count words (sauf ce qui se trouve en YAML et éventuellement entre `% xxxx %` ==> ce sont mes notes qui ne « sticky » pas).
    
   ##  5_scenes/
		1.1
		1.2 Etc. 
		
     5.2. Fragments --> sticky doit les chopper 
     
     
    
	## 6_chapitres/chapitrage, comme les scènes mais en chapitres, count par chapitre 
	
	##  7/Manuscrit >> en un seul bloc, count total 
---
==Effet de conséquences où il est peut-être bien de profiter pour COH, prévoir s'il est possible de scoper le LLM sur 5, 6 ou 7 selon la question. Économique. À voir comment gérer ça sur la DB (3 sorties différentes ?) ou ne retire que des scènes ? *TO RESOLVE*== 
---
    ## 8_Hadronchapitres/manuscrit_full, > Manuscrit monobloc 
    
    
    8_HadronHadron/inbox, ?? Je ne sais pas, je ne travaille pas à la Poste ?
    8_HadronHadron/a_trier,
    8_HadronHadron/triees,
    8_HadronHadron/HLC 
    8_HadronHadron/documentation,
    
    ## Archives 
    
==Alors ça ne servira pas ou rarement, mais dans le doute :==

    8_archives/scenes_coupees,
    8_archives/anciens_chapitres,
    8_archives/anciens_manuscrits_full,
    8_archives/anciens_plans,
    8_archives/anciennes_fiches,
    8_archives/vrac_historique,