"""Ce que la chaîne vocale fait d'une phrase.

La propriété qui porte le milestone se vérifie ici : quand la phrase est un ordre
d'arrêt, **le modèle de langage n'est jamais consulté**. Le double d'agent enregistre
ses appels, donc l'essai constate un fait plutôt qu'un message.
"""

import pytest

from roboto_voice.pipeline import VoicePipeline


class FakeRobot:
    def __init__(self) -> None:
        self.stops = 0
        self.fail = False

    def stop(self) -> dict:
        self.stops += 1
        if self.fail:
            raise ConnectionError("API injoignable")
        return {"accepted": True, "message": "arrêt d'urgence engagé"}


class FakeAgent:
    def __init__(self, reply: str = "d'accord") -> None:
        self.prompts: list[str] = []
        self.reply = reply
        self.fail = False

    def send(self, text: str) -> str:
        self.prompts.append(text)
        if self.fail:
            raise RuntimeError("quota dépassé")
        return self.reply


class FakeSpeaker:
    def __init__(self) -> None:
        self.said: list[str] = []

    def say(self, text: str) -> None:
        self.said.append(text)


@pytest.fixture
def parts():
    robot, agent, speaker = FakeRobot(), FakeAgent(), FakeSpeaker()
    return robot, agent, speaker, VoicePipeline(robot, agent, speaker)


# --- Le court-circuit -------------------------------------------------------------------


def test_an_order_to_stop_never_reaches_the_model(parts):
    """La raison d'être de cette conception.

    Si l'arrêt passait par le modèle, il hériterait de sa latence, de ses limites de
    débit et de ses pannes. « stop » doit fonctionner quand tout le reste est en panne.
    """
    robot, agent, _, pipeline = parts

    pipeline.handle("stop")

    assert robot.stops == 1
    assert agent.prompts == [], "l'ordre d'arrêt est passé par le modèle"


def test_the_stop_is_confirmed_out_loud(parts):
    _, _, speaker, pipeline = parts

    pipeline.handle("arrête-toi")

    assert speaker.said, "l'arrêt n'a pas été confirmé à voix haute"
    assert "arrêt" in speaker.said[0].lower()


def test_a_stop_still_works_when_the_model_is_absent():
    # Sans clé API, la conversation est impossible mais l'arrêt doit rester intact :
    # une commande vocale qui ne sait plus arrêter est pire qu'une absence de commande
    # vocale.
    robot, speaker = FakeRobot(), FakeSpeaker()
    pipeline = VoicePipeline(robot, None, speaker)

    pipeline.handle("stop")

    assert robot.stops == 1


def test_a_failed_stop_is_announced_as_a_failure(parts):
    robot, _, speaker, pipeline = parts
    robot.fail = True

    answer = pipeline.handle("stop")

    assert "non confirmé" in answer.lower() or "échec" in answer.lower()
    assert any("urgence" in said.lower() for said in speaker.said)


# --- La voie ordinaire -------------------------------------------------------------------


def test_an_ordinary_sentence_goes_to_the_model(parts):
    robot, agent, speaker, pipeline = parts

    pipeline.handle("où es-tu ?")

    assert agent.prompts == ["où es-tu ?"]
    assert robot.stops == 0
    assert speaker.said == ["d'accord"]


def test_without_a_model_an_ordinary_sentence_says_so():
    robot, speaker = FakeRobot(), FakeSpeaker()
    pipeline = VoicePipeline(robot, None, speaker)

    answer = pipeline.handle("va dans la cuisine")

    assert robot.stops == 0
    # Dire clairement que seule la commande d'arrêt est active, plutôt que de rester muet.
    assert "arrêt" in answer.lower()


def test_a_failing_model_does_not_break_the_stop(parts):
    robot, agent, _, pipeline = parts
    agent.fail = True

    pipeline.handle("raconte-moi une histoire")
    pipeline.handle("stop")

    assert robot.stops == 1


def test_a_failing_model_is_reported_not_swallowed(parts):
    _, agent, _, pipeline = parts
    agent.fail = True

    answer = pipeline.handle("bonjour")

    assert "quota" in answer.lower() or "erreur" in answer.lower()


def test_silence_does_nothing(parts):
    robot, agent, speaker, pipeline = parts

    pipeline.handle("")
    pipeline.handle("   ")

    assert robot.stops == 0
    assert agent.prompts == []
    assert speaker.said == []


# --- Trace -------------------------------------------------------------------------------


def test_every_utterance_is_recorded(parts):
    _, _, _, pipeline = parts

    pipeline.handle("stop")
    pipeline.handle("où es-tu ?")

    assert [entry.transcript for entry in pipeline.history] == ["stop", "où es-tu ?"]
    assert pipeline.history[0].urgent is True
    assert pipeline.history[1].urgent is False
