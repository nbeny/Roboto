"""Le court-circuit d'arrêt.

C'est la pièce la plus importante de la couche vocale, et la plus simple : une fonction
pure, sans réseau, sans modèle, sans robot. Elle décide si une phrase doit arrêter le
robot **avant** que quoi que ce soit d'intelligent n'ait son mot à dire.
"""

import pytest

from roboto_voice.urgent import is_emergency, normalise


# --- Ce qui doit arrêter ---------------------------------------------------------------


@pytest.mark.parametrize(
    "phrase",
    [
        "stop",
        "STOP",
        "Stop !",
        "arrête",
        "arrete",
        "arrête-toi",
        "arrêtes-toi tout de suite",
        "arrêtez",
        "stoppe",
        "au secours",
        "attention",
        "urgence",
        "danger",
        "halte",
    ],
)
def test_an_order_to_stop_is_recognised(phrase):
    assert is_emergency(phrase)


def test_an_order_buried_in_a_sentence_is_still_recognised():
    assert is_emergency("non mais attends, stop, il y a le chat")


def test_the_transcription_may_arrive_without_accents():
    # Whisper rend parfois « arrete », parfois « arrête », selon la qualité du son.
    # Dépendre de l'accent rendrait l'arrêt aléatoire.
    assert is_emergency("arrete toi")


def test_a_negated_order_still_stops():
    """Le choix de conception qui définit ce module.

    « ne t'arrête pas » contient « arrête », et déclenche donc l'arrêt. C'est délibéré.

    L'asymétrie est totale : un arrêt inutile coûte quelques secondes et une reprise
    explicite ; un arrêt manqué coûte potentiellement quelqu'un. Analyser la négation
    pour éviter le premier reviendrait à accepter le second, contre un gain sans commune
    mesure.
    """
    assert is_emergency("ne t'arrête pas")
    assert is_emergency("surtout ne stoppe pas")


# --- Ce qui ne doit pas arrêter ---------------------------------------------------------


@pytest.mark.parametrize(
    "phrase",
    [
        "va dans la cuisine",
        "où es-tu ?",
        "quelle est ta position",
        "avance de deux mètres",
        "que vois-tu devant toi",
        "tourne à gauche",
        "",
        "   ",
    ],
)
def test_an_ordinary_sentence_does_not_stop(phrase):
    # Le pendant indispensable : un détecteur qui arrête à tout propos rend la commande
    # vocale inutilisable, et l'utilisateur finit par la désactiver — ce qui supprime
    # aussi le vrai arrêt.
    assert not is_emergency(phrase)


def test_nothing_at_all_does_not_stop():
    assert not is_emergency(None)


def test_a_word_that_merely_contains_a_keyword_does_not_stop():
    # « stopper » déclenche, c'est voulu. Mais un mot sans rapport qui contiendrait la
    # suite de lettres ne doit pas : la correspondance porte sur des mots, pas sur des
    # sous-chaînes quelconques.
    assert not is_emergency("les astrophysiciens")
    assert not is_emergency("la charrette")


# --- Normalisation ----------------------------------------------------------------------


def test_normalise_strips_accents_case_and_punctuation():
    assert normalise("Arrête-toi, STOP !") == "arrete toi stop"


def test_normalise_handles_nothing():
    assert normalise(None) == ""
    assert normalise("") == ""


# --- Coût -------------------------------------------------------------------------------


def test_the_check_is_pure_and_immediate():
    """Aucun réseau, aucun modèle, aucun état.

    C'est ce qui permet de la placer avant tout le reste : si elle pouvait échouer,
    attendre ou dépendre d'un service, elle ne serait pas un court-circuit.
    """
    import inspect

    import roboto_voice.urgent as module

    source = inspect.getsource(module)
    for forbidden in ("import requests", "urllib", "anthropic", "socket", "http"):
        assert forbidden not in source, f"le court-circuit dépend de {forbidden}"
